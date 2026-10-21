//! Type inference for one body.

use std::collections::HashSet;

use flux_arena::ArenaMap;
use flux_hir::{
    ArithOp, BinaryOp, Binding, BindingId, Body, BodySourceMap, CmpOp, Diagnostic, Expr, ExprId,
    Function, FunctionId, ItemTree, Literal, LogicOp, LoopSource, Pat, PatId, Res, Stmt, TypeRefId,
    UnaryOp,
};
use flux_intern::Symbol;
use flux_syntax::TextRange;

use crate::ty::{InferVar, IntTy, Mutability, Ty, TyKind};
use crate::{FnSig, function_name, resolve_or_report};

/// The types of one body.
#[derive(Debug)]
pub struct InferenceResult {
    /// The type of every expression, except the callee of a direct call
    /// (`f` in `f(x)`), which has no value of its own.
    pub expr_types: ArenaMap<Expr, Ty>,
    pub binding_types: ArenaMap<Binding, Ty>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Infers the types of `function`'s body.
pub fn infer_body(
    items: &ItemTree,
    sigs: &ArenaMap<Function, FnSig>,
    function: FunctionId,
    body: &Body,
    source_map: &BodySourceMap,
) -> InferenceResult {
    let sig = &sigs[function];
    let mut ctx = Ctx {
        items,
        sigs,
        body,
        source_map,
        vars: Vec::new(),
        expr_types: ArenaMap::new(),
        binding_types: ArenaMap::new(),
        loops: ArenaMap::new(),
        uninit_bindings: HashSet::new(),
        unsafe_depth: u32::from(items.functions[function].is_unsafe),
        ret: sig.ret,
        deferred: Vec::new(),
        diagnostics: Vec::new(),
    };
    for (i, &pat) in body.params.iter().enumerate() {
        let ty = sig.params.get(i).copied().unwrap_or(Ty::ERROR);
        ctx.bind_pat(pat, ty, false);
    }
    let ty = ctx.infer_expr(body.root);
    ctx.coerce(ty, sig.ret, body.root);
    ctx.finish()
}

/// The type of `c"..."`: `&'static CStr`.
pub fn c_str_ref() -> Ty {
    Ty::new(TyKind::Ref(Mutability::Not, Ty::new(TyKind::CStr)))
}

/// What `CStr::as_ptr` returns: `*const c_char`, which is `*const i8` on
/// both targets flux supports.
pub fn c_char_ptr() -> Ty {
    Ty::raw_ptr(Mutability::Not, Ty::int(IntTy::I8))
}

#[derive(Debug, Clone, Copy)]
enum VarValue {
    Unknown {
        /// Created for an unsuffixed integer literal: unifies only with
        /// integer types, and defaults to `i32`.
        int: bool,
        /// Fed a `!` by coercion: falls back to `!` if nothing else
        /// constrains it.
        diverging: bool,
    },
    Known(Ty),
    Link(InferVar),
}

#[derive(Debug, Clone, Copy)]
struct LoopState {
    ty: Ty,
    broken: bool,
    source: LoopSource,
}

/// A check that needs the final types.
#[derive(Debug, Clone, Copy)]
enum Deferred {
    IntLiteral {
        expr: ExprId,
        value: u128,
    },
    Unary {
        expr: ExprId,
        op: UnaryOp,
        ty: Ty,
    },
    Binary {
        expr: ExprId,
        op: BinaryOp,
        ty: Ty,
    },
    /// The right operand of `<<` or `>>`, which may be any integer type.
    ShiftAmount {
        expr: ExprId,
        ty: Ty,
    },
    Cast {
        expr: ExprId,
        from: Ty,
        to: Ty,
    },
    /// An argument passed through a variadic function's `...`.
    VariadicArg {
        expr: ExprId,
        ty: Ty,
    },
}

struct Ctx<'a> {
    items: &'a ItemTree,
    sigs: &'a ArenaMap<Function, FnSig>,
    body: &'a Body,
    source_map: &'a BodySourceMap,
    /// Each variable's value and where it was created.
    vars: Vec<(VarValue, TextRange)>,
    expr_types: ArenaMap<Expr, Ty>,
    binding_types: ArenaMap<Binding, Ty>,
    loops: ArenaMap<Expr, LoopState>,
    /// `let x;` bindings, which may be assigned once without `mut`.
    uninit_bindings: HashSet<BindingId>,
    unsafe_depth: u32,
    ret: Ty,
    deferred: Vec<Deferred>,
    diagnostics: Vec<Diagnostic>,
}

impl Ctx<'_> {
    fn range(&self, expr: ExprId) -> TextRange {
        self.source_map.exprs.get(expr).copied().unwrap_or_default()
    }

    fn error(&mut self, range: TextRange, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            range,
            message: message.into(),
        });
    }

    // Variables and unification.

    fn new_var(&mut self, int: bool, origin: TextRange) -> Ty {
        let var = InferVar(self.vars.len() as u32);
        self.vars.push((
            VarValue::Unknown {
                int,
                diverging: false,
            },
            origin,
        ));
        Ty::infer(var)
    }

    fn root(&mut self, var: InferVar) -> InferVar {
        match self.vars[var.0 as usize].0 {
            VarValue::Link(next) => {
                let root = self.root(next);
                self.vars[var.0 as usize].0 = VarValue::Link(root);
                root
            }
            _ => var,
        }
    }

    fn unknown(&self, var: InferVar) -> (bool, bool) {
        match self.vars[var.0 as usize].0 {
            VarValue::Unknown { int, diverging } => (int, diverging),
            _ => unreachable!("roots of unresolved variables are unknown"),
        }
    }

    /// `ty` with a known variable at the top replaced by its value.
    fn shallow(&mut self, ty: Ty) -> Ty {
        let TyKind::Infer(var) = ty.kind() else {
            return ty;
        };
        let root = self.root(var);
        match self.vars[root.0 as usize].0 {
            VarValue::Known(ty) => self.shallow(ty),
            _ => Ty::infer(root),
        }
    }

    fn unify(&mut self, a: Ty, b: Ty) -> bool {
        let (a, b) = (self.shallow(a), self.shallow(b));
        if a == b {
            return true;
        }
        match (a.kind(), b.kind()) {
            (TyKind::Error, _) | (_, TyKind::Error) => true,
            (TyKind::Infer(x), TyKind::Infer(y)) => {
                let ((xi, xd), (yi, yd)) = (self.unknown(x), self.unknown(y));
                self.vars[x.0 as usize].0 = VarValue::Link(y);
                self.vars[y.0 as usize].0 = VarValue::Unknown {
                    int: xi || yi,
                    diverging: xd || yd,
                };
                true
            }
            (TyKind::Infer(var), _) => self.bind_var(var, b),
            (_, TyKind::Infer(var)) => self.bind_var(var, a),
            (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y))
            | (TyKind::Ref(m, x), TyKind::Ref(n, y)) => m == n && self.unify(x, y),
            _ => false,
        }
    }

    fn bind_var(&mut self, var: InferVar, ty: Ty) -> bool {
        let (int, _) = self.unknown(var);
        if int && ty.as_int().is_none() {
            return false;
        }
        self.vars[var.0 as usize].0 = VarValue::Known(ty);
        true
    }

    /// Checks that a value of type `actual` (from `expr`) can be used where
    /// `expected` is: `!` coerces to anything, other types must unify.
    fn coerce(&mut self, actual: Ty, expected: Ty, expr: ExprId) {
        if self.shallow(actual) == Ty::NEVER {
            if let TyKind::Infer(var) = self.shallow(expected).kind() {
                let (int, _) = self.unknown(var);
                self.vars[var.0 as usize].0 = VarValue::Unknown {
                    int,
                    diverging: true,
                };
            }
            return;
        }
        if !self.unify(actual, expected) {
            let (expected, actual) = (self.describe(expected), self.describe(actual));
            let range = self.range(self.blame(expr));
            self.error(
                range,
                format!("mismatched types: expected `{expected}`, found `{actual}`"),
            );
        }
    }

    /// The expression a type mismatch in `expr` is best reported at: the
    /// value a block ends with rather than the whole block.
    fn blame(&self, expr: ExprId) -> ExprId {
        match self.body.exprs[expr] {
            Expr::Block {
                tail: Some(tail), ..
            } => self.blame(tail),
            _ => expr,
        }
    }

    /// A type for messages: known variables replaced, `{integer}` for an
    /// integer variable and `_` for any other.
    fn describe(&mut self, ty: Ty) -> String {
        let ty = self.shallow(ty);
        match ty.kind() {
            TyKind::Infer(var) => match self.unknown(var) {
                (true, _) => "{integer}".to_owned(),
                _ => "_".to_owned(),
            },
            TyKind::RawPtr(m, pointee) => {
                let m = if m == Mutability::Mut { "mut" } else { "const" };
                format!("*{m} {}", self.describe(pointee))
            }
            TyKind::Ref(m, pointee) => {
                let m = if m == Mutability::Mut { "mut " } else { "" };
                format!("&{m}{}", self.describe(pointee))
            }
            _ => ty.to_string(),
        }
    }

    fn resolve_type_ref(&mut self, ty: TypeRefId) -> Ty {
        let range = self.source_map.types.get(ty).copied().unwrap_or_default();
        resolve_or_report(&self.body.types[ty], range, &mut self.diagnostics)
    }

    fn bind_pat(&mut self, pat: PatId, ty: Ty, uninit: bool) {
        if let Pat::Bind(binding) = self.body.pats[pat] {
            self.binding_types.insert(binding, ty);
            if uninit {
                self.uninit_bindings.insert(binding);
            }
        }
    }

    // Expressions.

    fn infer_expr(&mut self, id: ExprId) -> Ty {
        let ty = self.infer_expr_inner(id);
        self.expr_types.insert(id, ty);
        ty
    }

    fn infer_expr_inner(&mut self, id: ExprId) -> Ty {
        let body = self.body;
        match &body.exprs[id] {
            Expr::Missing => Ty::ERROR,
            Expr::Literal(Literal::Bool(_)) => Ty::BOOL,
            Expr::Literal(Literal::CStr(_)) => c_str_ref(),
            Expr::Literal(Literal::Int { value, suffix }) => {
                self.deferred.push(Deferred::IntLiteral {
                    expr: id,
                    value: *value,
                });
                match suffix.and_then(IntTy::from_name) {
                    Some(int) => Ty::int(int),
                    None => self.new_var(true, self.range(id)),
                }
            }
            Expr::Path(Res::Local(binding)) => self
                .binding_types
                .get(*binding)
                .copied()
                .unwrap_or(Ty::ERROR),
            Expr::Path(Res::Function(_)) => {
                self.error(self.range(id), "functions as values are not supported yet");
                Ty::ERROR
            }
            Expr::Path(Res::Err) => Ty::ERROR,
            Expr::Tuple(fields) => {
                for &field in fields.iter() {
                    self.infer_expr(field);
                }
                if fields.is_empty() {
                    Ty::UNIT
                } else {
                    self.error(self.range(id), "tuples are not supported yet");
                    Ty::ERROR
                }
            }
            Expr::Block {
                stmts,
                tail,
                is_unsafe,
            } => {
                self.unsafe_depth += u32::from(*is_unsafe);
                let ty = self.infer_block(stmts, *tail);
                self.unsafe_depth -= u32::from(*is_unsafe);
                ty
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.infer_expr(*condition);
                self.coerce(cond, Ty::BOOL, *condition);
                let then_ty = self.infer_expr(*then_branch);
                match else_branch {
                    None => {
                        self.coerce(then_ty, Ty::UNIT, *then_branch);
                        Ty::UNIT
                    }
                    Some(else_branch) => {
                        let else_ty = self.infer_expr(*else_branch);
                        self.join(then_ty, else_ty, id)
                    }
                }
            }
            Expr::Loop {
                body: loop_body,
                source,
            } => {
                let ty = match source {
                    LoopSource::While => Ty::UNIT,
                    LoopSource::Loop => self.new_var(false, self.range(id)),
                };
                self.loops.insert(
                    id,
                    LoopState {
                        ty,
                        broken: false,
                        source: *source,
                    },
                );
                let body_ty = self.infer_expr(*loop_body);
                self.coerce(body_ty, Ty::UNIT, *loop_body);
                let state = self.loops.remove(id).expect("inserted above");
                match source {
                    LoopSource::While => Ty::UNIT,
                    // A `loop` that nothing breaks out of never finishes.
                    LoopSource::Loop if state.broken => ty,
                    LoopSource::Loop => Ty::NEVER,
                }
            }
            Expr::Break { value, target } => {
                let value_ty = value.map(|v| self.infer_expr(v));
                let state = target.and_then(|t| self.loops.get(t).copied());
                if let (Some(target), Some(state)) = (*target, state) {
                    if state.source == LoopSource::While && value.is_some() {
                        self.error(self.range(id), "`break` with value from a `while` loop");
                    } else {
                        self.coerce(value_ty.unwrap_or(Ty::UNIT), state.ty, value.unwrap_or(id));
                    }
                    if let Some(state) = self.loops.get_mut(target) {
                        state.broken = true;
                    }
                }
                Ty::NEVER
            }
            Expr::Continue { .. } => Ty::NEVER,
            Expr::Return { value } => {
                let ty = match value {
                    Some(value) => self.infer_expr(*value),
                    None => Ty::UNIT,
                };
                self.coerce(ty, self.ret, value.unwrap_or(id));
                Ty::NEVER
            }
            Expr::Call { callee, args } => self.infer_call(id, *callee, args),
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => self.infer_method_call(id, *receiver, *method, args),
            Expr::Unary { op, operand } => {
                let ty = self.infer_expr(*operand);
                if *op == UnaryOp::Deref {
                    self.error(self.range(id), "dereferencing is not supported yet");
                    return Ty::ERROR;
                }
                self.deferred.push(Deferred::Unary {
                    expr: id,
                    op: *op,
                    ty,
                });
                ty
            }
            Expr::Binary { op, lhs, rhs } => self.infer_binary(id, *op, *lhs, *rhs),
            Expr::Cast { expr, ty } => {
                let to = self.resolve_type_ref(*ty);
                let from = self.infer_expr(*expr);
                // As in rustc, an unsuffixed literal cast to an integer type
                // has that type: `300 as u8` is out of range, not 44.
                let bare_literal = matches!(
                    body.exprs[*expr],
                    Expr::Literal(Literal::Int { suffix: None, .. })
                );
                if bare_literal && to.as_int().is_some() {
                    self.unify(from, to);
                }
                self.deferred.push(Deferred::Cast { expr: id, from, to });
                to
            }
        }
    }

    fn infer_block(&mut self, stmts: &[Stmt], tail: Option<ExprId>) -> Ty {
        let mut diverges = false;
        for stmt in stmts {
            match *stmt {
                Stmt::Let { pat, ty, init } => {
                    let declared = ty.map(|ty| self.resolve_type_ref(ty));
                    let ty = match (declared, init) {
                        (Some(declared), Some(init)) => {
                            let ty = self.infer_expr(init);
                            self.coerce(ty, declared, init);
                            diverges |= self.shallow(ty) == Ty::NEVER;
                            declared
                        }
                        (None, Some(init)) => {
                            let ty = self.infer_expr(init);
                            diverges |= self.shallow(ty) == Ty::NEVER;
                            ty
                        }
                        (Some(declared), None) => declared,
                        (None, None) => {
                            let range = self.source_map.pats.get(pat).copied();
                            self.new_var(false, range.unwrap_or_default())
                        }
                    };
                    self.bind_pat(pat, ty, init.is_none());
                }
                Stmt::Expr {
                    expr,
                    has_semicolon,
                } => {
                    let ty = self.infer_expr(expr);
                    // A block-like statement without `;` must be `()`.
                    if !has_semicolon {
                        self.coerce(ty, Ty::UNIT, expr);
                    }
                    diverges |= self.shallow(ty) == Ty::NEVER;
                }
            }
        }
        match tail {
            Some(tail) => self.infer_expr(tail),
            None if diverges => Ty::NEVER,
            None => Ty::UNIT,
        }
    }

    /// The type of an `if` with both branches: the branches' common type,
    /// where a `!` branch takes the other's type.
    fn join(&mut self, then_ty: Ty, else_ty: Ty, expr: ExprId) -> Ty {
        if self.shallow(then_ty) == Ty::NEVER {
            return else_ty;
        }
        if self.shallow(else_ty) == Ty::NEVER || self.unify(then_ty, else_ty) {
            return then_ty;
        }
        let (a, b) = (self.describe(then_ty), self.describe(else_ty));
        self.error(
            self.range(expr),
            format!("`if` and `else` have incompatible types: `{a}` and `{b}`"),
        );
        Ty::ERROR
    }

    fn infer_args(&mut self, args: &[ExprId]) {
        for &arg in args {
            self.infer_expr(arg);
        }
    }

    fn infer_call(&mut self, id: ExprId, callee: ExprId, args: &[ExprId]) -> Ty {
        let function = match self.body.exprs[callee] {
            Expr::Path(Res::Function(function)) => function,
            Expr::Path(Res::Err) | Expr::Missing => {
                self.infer_args(args);
                return Ty::ERROR;
            }
            _ => {
                let ty = self.infer_expr(callee);
                let ty = self.describe(ty);
                self.error(
                    self.range(callee),
                    format!("expected function, found `{ty}`"),
                );
                self.infer_args(args);
                return Ty::ERROR;
            }
        };
        let sig = &self.sigs[function];
        let data = &self.items.functions[function];
        let name = function_name(self.items, function);
        let (want, got) = (sig.params.len(), args.len());
        let arity_ok = if data.is_variadic {
            got >= want
        } else {
            got == want
        };
        if !arity_ok {
            let at_least = if data.is_variadic { "at least " } else { "" };
            let plural = |n: usize| if n == 1 { "argument" } else { "arguments" };
            let verb = if got == 1 { "was" } else { "were" };
            self.error(
                self.range(id),
                format!(
                    "`{name}` takes {at_least}{want} {} but {got} {} {verb} supplied",
                    plural(want),
                    plural(got)
                ),
            );
        }
        for (i, &arg) in args.iter().enumerate() {
            let ty = self.infer_expr(arg);
            match sig.params.get(i) {
                Some(&param) => self.coerce(ty, param, arg),
                None if data.is_variadic => {
                    self.deferred.push(Deferred::VariadicArg { expr: arg, ty });
                }
                None => {}
            }
        }
        if data.is_unsafe && self.unsafe_depth == 0 {
            self.error(
                self.range(id),
                format!("call to unsafe function `{name}` requires an `unsafe` block"),
            );
        }
        sig.ret
    }

    /// Only `CStr::as_ptr` exists so far, until flux has `impl` blocks and
    /// its own core library.
    fn infer_method_call(
        &mut self,
        id: ExprId,
        receiver: ExprId,
        method: Symbol,
        args: &[ExprId],
    ) -> Ty {
        let receiver_ty = self.infer_expr(receiver);
        self.infer_args(args);
        let receiver_ty = self.shallow(receiver_ty);
        if receiver_ty == Ty::ERROR {
            return Ty::ERROR;
        }
        if receiver_ty == c_str_ref() && method.as_str() == "as_ptr" {
            if !args.is_empty() {
                let got = args.len();
                let verb = if got == 1 {
                    "argument was"
                } else {
                    "arguments were"
                };
                self.error(
                    self.range(id),
                    format!("`as_ptr` takes 0 arguments but {got} {verb} supplied"),
                );
            }
            return c_char_ptr();
        }
        let ty = self.describe(receiver_ty);
        self.error(
            self.range(id),
            format!("the method `{method}` on `{ty}` is not supported yet"),
        );
        Ty::ERROR
    }

    fn infer_binary(&mut self, id: ExprId, op: BinaryOp, lhs: ExprId, rhs: ExprId) -> Ty {
        match op {
            BinaryOp::Logic(_) => {
                for operand in [lhs, rhs] {
                    let ty = self.infer_expr(operand);
                    self.coerce(ty, Ty::BOOL, operand);
                }
                Ty::BOOL
            }
            BinaryOp::Cmp(_) => {
                let ty = self.infer_expr(lhs);
                let rhs_ty = self.infer_expr(rhs);
                self.coerce(rhs_ty, ty, rhs);
                self.deferred.push(Deferred::Binary { expr: id, op, ty });
                Ty::BOOL
            }
            BinaryOp::Arith(arith) => self.infer_arith(id, op, arith, lhs, rhs),
            BinaryOp::Assign(arith) => {
                self.check_place(lhs, arith.is_none());
                let ty = self.infer_expr(lhs);
                match arith {
                    None => {
                        let rhs_ty = self.infer_expr(rhs);
                        self.coerce(rhs_ty, ty, rhs);
                    }
                    Some(arith) => {
                        self.infer_arith(id, op, arith, lhs, rhs);
                    }
                }
                Ty::UNIT
            }
        }
    }

    /// `lhs op rhs` for an arithmetic or bitwise `op`, also the compound
    /// assignment `lhs op= rhs` (then `lhs` was already inferred).
    fn infer_arith(
        &mut self,
        id: ExprId,
        op: BinaryOp,
        arith: ArithOp,
        lhs: ExprId,
        rhs: ExprId,
    ) -> Ty {
        let ty = match self.expr_types.get(lhs) {
            Some(&ty) => ty,
            None => self.infer_expr(lhs),
        };
        let rhs_ty = self.infer_expr(rhs);
        if matches!(arith, ArithOp::Shl | ArithOp::Shr) {
            self.deferred.push(Deferred::ShiftAmount {
                expr: rhs,
                ty: rhs_ty,
            });
        } else {
            self.coerce(rhs_ty, ty, rhs);
        }
        self.deferred.push(Deferred::Binary { expr: id, op, ty });
        ty
    }

    /// The left side of an assignment must be a local; one without `mut`
    /// only if it was declared without a value (`let x; x = 1;`).
    fn check_place(&mut self, lhs: ExprId, plain: bool) {
        match self.body.exprs[lhs] {
            Expr::Path(Res::Local(binding)) => {
                let data = &self.body.bindings[binding];
                let first_assignment = plain && self.uninit_bindings.contains(&binding);
                if !data.mutable && !first_assignment {
                    let name = data.name;
                    self.error(
                        self.range(lhs),
                        format!("cannot assign twice to immutable variable `{name}`"),
                    );
                }
            }
            Expr::Path(Res::Err) | Expr::Missing => {}
            _ => self.error(self.range(lhs), "invalid left-hand side of assignment"),
        }
    }

    // Finishing.

    fn finish(mut self) -> InferenceResult {
        let mut reported = HashSet::new();
        let exprs: Vec<(ExprId, Ty)> = self.expr_types.iter().map(|(e, t)| (e, *t)).collect();
        for (expr, ty) in exprs {
            let ty = self.resolve(ty, &mut reported);
            self.expr_types.insert(expr, ty);
        }
        let bindings: Vec<(BindingId, Ty)> =
            self.binding_types.iter().map(|(b, t)| (b, *t)).collect();
        for (binding, ty) in bindings {
            let ty = self.resolve(ty, &mut reported);
            self.binding_types.insert(binding, ty);
        }
        self.run_deferred(&mut reported);
        InferenceResult {
            expr_types: self.expr_types,
            binding_types: self.binding_types,
            diagnostics: self.diagnostics,
        }
    }

    /// `ty` with every variable replaced, at any depth: integer variables
    /// default to `i32`, diverging ones to `!`, and any other unknown
    /// variable is reported once and becomes an error.
    fn resolve(&mut self, ty: Ty, reported: &mut HashSet<InferVar>) -> Ty {
        let ty = self.shallow(ty);
        match ty.kind() {
            TyKind::Infer(var) => {
                let (value, origin) = self.vars[var.0 as usize];
                let ty = match value {
                    VarValue::Unknown { int: true, .. } => Ty::int(IntTy::I32),
                    VarValue::Unknown {
                        diverging: true, ..
                    } => Ty::NEVER,
                    _ => {
                        if reported.insert(var) {
                            self.error(origin, "type annotations needed");
                        }
                        Ty::ERROR
                    }
                };
                self.vars[var.0 as usize].0 = VarValue::Known(ty);
                ty
            }
            TyKind::RawPtr(m, pointee) => Ty::raw_ptr(m, self.resolve(pointee, reported)),
            TyKind::Ref(m, pointee) => Ty::new(TyKind::Ref(m, self.resolve(pointee, reported))),
            _ => ty,
        }
    }

    fn run_deferred(&mut self, reported: &mut HashSet<InferVar>) {
        // Operands of `-`, where a literal may reach one past the maximum.
        let negated: HashSet<ExprId> = self
            .body
            .exprs
            .iter()
            .filter_map(|(_, e)| match e {
                Expr::Unary {
                    op: UnaryOp::Neg,
                    operand,
                } => Some(*operand),
                _ => None,
            })
            .collect();
        for deferred in std::mem::take(&mut self.deferred) {
            match deferred {
                Deferred::IntLiteral { expr, value } => {
                    let Some(int) = self.expr_types.get(expr).and_then(|ty| ty.as_int()) else {
                        continue;
                    };
                    let max = int.max();
                    let limit = if negated.contains(&expr) && int.is_signed() {
                        max + 1
                    } else {
                        max
                    };
                    if value > limit {
                        self.error(
                            self.range(expr),
                            format!("literal out of range for `{int}`", int = int.name()),
                        );
                    }
                }
                Deferred::Unary { expr, op, ty } => {
                    let ty = self.resolve(ty, reported);
                    let ok = match (op, ty.kind()) {
                        (_, TyKind::Error | TyKind::Never) => true,
                        (UnaryOp::Neg, TyKind::Int(int)) => int.is_signed(),
                        (UnaryOp::Not, TyKind::Int(_) | TyKind::Bool) => true,
                        _ => false,
                    };
                    if !ok {
                        let op = if op == UnaryOp::Neg { "-" } else { "!" };
                        self.error(
                            self.range(expr),
                            format!("cannot apply unary operator `{op}` to type `{ty}`"),
                        );
                    }
                }
                Deferred::Binary { expr, op, ty } => {
                    let ty = self.resolve(ty, reported);
                    let ok = match (op, ty.kind()) {
                        (_, TyKind::Error | TyKind::Never) => true,
                        (BinaryOp::Cmp(_), TyKind::Int(_) | TyKind::Bool | TyKind::Unit) => true,
                        (BinaryOp::Arith(op) | BinaryOp::Assign(Some(op)), TyKind::Bool) => {
                            matches!(op, ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor)
                        }
                        (_, TyKind::Int(_)) => true,
                        _ => false,
                    };
                    if !ok {
                        self.error(
                            self.range(expr),
                            format!(
                                "cannot apply binary operator `{}` to type `{ty}`",
                                op_text(op)
                            ),
                        );
                    }
                }
                Deferred::ShiftAmount { expr, ty } => {
                    let ty = self.resolve(ty, reported);
                    if !matches!(ty.kind(), TyKind::Int(_) | TyKind::Error | TyKind::Never) {
                        self.error(
                            self.range(expr),
                            format!("cannot shift by a value of type `{ty}`"),
                        );
                    }
                }
                Deferred::Cast { expr, from, to } => {
                    let from = self.resolve(from, reported);
                    let ok = match (from.kind(), to.kind()) {
                        (TyKind::Error | TyKind::Never, _) | (_, TyKind::Error) => true,
                        (TyKind::Int(_) | TyKind::Bool, TyKind::Int(_)) => true,
                        _ => from == to,
                    };
                    if !ok {
                        let message = if from.is_pointer() || to.is_pointer() {
                            "casts of pointers are not supported yet".to_owned()
                        } else if to == Ty::BOOL && from.as_int().is_some() {
                            format!("cannot cast `{from}` as `bool`")
                        } else {
                            format!("non-primitive cast: `{from}` as `{to}`")
                        };
                        self.error(self.range(expr), message);
                    }
                }
                Deferred::VariadicArg { expr, ty } => {
                    let ty = self.resolve(ty, reported);
                    // C promotes narrower arguments to `int`, and rustc makes
                    // the caller write that conversion.
                    let cast_to = match ty.kind() {
                        TyKind::Int(int) if int.bits() < 32 => {
                            Some(if int.is_signed() { "c_int" } else { "c_uint" })
                        }
                        TyKind::Bool => Some("c_int"),
                        TyKind::Int(int) if int.bits() == 128 => {
                            self.error(
                                self.range(expr),
                                format!("passing `{ty}` to a variadic function is not supported"),
                            );
                            None
                        }
                        _ => None,
                    };
                    let ok = matches!(
                        ty.kind(),
                        TyKind::Int(_) | TyKind::RawPtr(..) | TyKind::Error | TyKind::Never
                    );
                    if let Some(cast_to) = cast_to {
                        self.error(
                            self.range(expr),
                            format!(
                                "can't pass `{ty}` to variadic function; cast it to `{cast_to}`"
                            ),
                        );
                    } else if !ok {
                        self.error(
                            self.range(expr),
                            format!("can't pass `{ty}` to variadic function"),
                        );
                    }
                }
            }
        }
    }
}

fn op_text(op: BinaryOp) -> &'static str {
    let arith = |op: ArithOp| match op {
        ArithOp::Add => "+",
        ArithOp::Sub => "-",
        ArithOp::Mul => "*",
        ArithOp::Div => "/",
        ArithOp::Rem => "%",
        ArithOp::Shl => "<<",
        ArithOp::Shr => ">>",
        ArithOp::BitXor => "^",
        ArithOp::BitOr => "|",
        ArithOp::BitAnd => "&",
    };
    match op {
        BinaryOp::Logic(LogicOp::And) => "&&",
        BinaryOp::Logic(LogicOp::Or) => "||",
        BinaryOp::Arith(op) => arith(op),
        BinaryOp::Assign(Some(op)) => match op {
            ArithOp::Add => "+=",
            ArithOp::Sub => "-=",
            ArithOp::Mul => "*=",
            ArithOp::Div => "/=",
            ArithOp::Rem => "%=",
            ArithOp::Shl => "<<=",
            ArithOp::Shr => ">>=",
            ArithOp::BitXor => "^=",
            ArithOp::BitOr => "|=",
            ArithOp::BitAnd => "&=",
        },
        BinaryOp::Assign(None) => "=",
        BinaryOp::Cmp(op) => match op {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        },
    }
}
