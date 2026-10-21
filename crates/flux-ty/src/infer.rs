//! Type inference for one body.

use std::collections::HashSet;

use flux_arena::ArenaMap;
use flux_hir::{
    ArithOp, BinaryOp, Binding, BindingId, Body, BodySourceMap, CmpOp, Diagnostic, Expr, ExprId,
    Function, FunctionId, ItemTree, Literal, LogicOp, LoopSource, Pat, PatId, Res, Stmt, TypeRefId,
    UnaryOp,
};
use flux_syntax::TextRange;

use crate::ty::{InferVar, IntTy, Ty};
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
        let ty = sig.params.get(i).copied().unwrap_or(Ty::Error);
        ctx.bind_pat(pat, ty, false);
    }
    let ty = ctx.infer_expr(body.root);
    ctx.coerce(ty, sig.ret, body.root);
    ctx.finish()
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
        Ty::Infer(var)
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

    /// `ty` with known variables replaced, one level deep (types have no
    /// structure yet, so that is all of it).
    fn shallow(&mut self, ty: Ty) -> Ty {
        let Ty::Infer(var) = ty else {
            return ty;
        };
        let root = self.root(var);
        match self.vars[root.0 as usize].0 {
            VarValue::Known(ty) => ty,
            _ => Ty::Infer(root),
        }
    }

    fn unify(&mut self, a: Ty, b: Ty) -> bool {
        let (a, b) = (self.shallow(a), self.shallow(b));
        match (a, b) {
            _ if a == b => true,
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::Infer(x), Ty::Infer(y)) => {
                let (
                    VarValue::Unknown {
                        int: xi,
                        diverging: xd,
                    },
                    _,
                ) = self.vars[x.0 as usize]
                else {
                    unreachable!("roots are unknown")
                };
                let (
                    VarValue::Unknown {
                        int: yi,
                        diverging: yd,
                    },
                    _,
                ) = self.vars[y.0 as usize]
                else {
                    unreachable!("roots are unknown")
                };
                self.vars[x.0 as usize].0 = VarValue::Link(y);
                self.vars[y.0 as usize].0 = VarValue::Unknown {
                    int: xi || yi,
                    diverging: xd || yd,
                };
                true
            }
            (Ty::Infer(var), ty) | (ty, Ty::Infer(var)) => {
                let VarValue::Unknown { int, .. } = self.vars[var.0 as usize].0 else {
                    unreachable!("roots are unknown")
                };
                if int && !matches!(ty, Ty::Int(_)) {
                    return false;
                }
                self.vars[var.0 as usize].0 = VarValue::Known(ty);
                true
            }
            _ => false,
        }
    }

    /// Checks that a value of type `actual` (from `expr`) can be used where
    /// `expected` is: `!` coerces to anything, other types must unify.
    fn coerce(&mut self, actual: Ty, expected: Ty, expr: ExprId) {
        if self.shallow(actual) == Ty::Never {
            if let Ty::Infer(var) = self.shallow(expected)
                && let VarValue::Unknown { int, .. } = self.vars[var.0 as usize].0
            {
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

    /// A type for messages: `{integer}` for an integer variable.
    fn describe(&mut self, ty: Ty) -> String {
        match self.shallow(ty) {
            Ty::Infer(var) => match self.vars[var.0 as usize].0 {
                VarValue::Unknown { int: true, .. } => "{integer}".to_owned(),
                _ => "_".to_owned(),
            },
            ty => ty.to_string(),
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
            Expr::Missing => Ty::Error,
            Expr::Literal(Literal::Bool(_)) => Ty::Bool,
            Expr::Literal(Literal::Int { value, suffix }) => {
                self.deferred.push(Deferred::IntLiteral {
                    expr: id,
                    value: *value,
                });
                match suffix.and_then(IntTy::from_name) {
                    Some(int) => Ty::Int(int),
                    None => self.new_var(true, self.range(id)),
                }
            }
            Expr::Path(Res::Local(binding)) => self
                .binding_types
                .get(*binding)
                .copied()
                .unwrap_or(Ty::Error),
            Expr::Path(Res::Function(_)) => {
                self.error(self.range(id), "functions as values are not supported yet");
                Ty::Error
            }
            Expr::Path(Res::Err) => Ty::Error,
            Expr::Tuple(fields) => {
                for &field in fields.iter() {
                    self.infer_expr(field);
                }
                if fields.is_empty() {
                    Ty::Unit
                } else {
                    self.error(self.range(id), "tuples are not supported yet");
                    Ty::Error
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
                self.coerce(cond, Ty::Bool, *condition);
                let then_ty = self.infer_expr(*then_branch);
                match else_branch {
                    None => {
                        self.coerce(then_ty, Ty::Unit, *then_branch);
                        Ty::Unit
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
                    LoopSource::While => Ty::Unit,
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
                self.coerce(body_ty, Ty::Unit, *loop_body);
                let state = self.loops.remove(id).expect("inserted above");
                match source {
                    LoopSource::While => Ty::Unit,
                    // A `loop` that nothing breaks out of never finishes.
                    LoopSource::Loop if state.broken => ty,
                    LoopSource::Loop => Ty::Never,
                }
            }
            Expr::Break { value, target } => {
                let value_ty = value.map(|v| self.infer_expr(v));
                let state = target.and_then(|t| self.loops.get(t).copied());
                if let (Some(target), Some(state)) = (*target, state) {
                    if state.source == LoopSource::While && value.is_some() {
                        self.error(self.range(id), "`break` with value from a `while` loop");
                    } else {
                        self.coerce(value_ty.unwrap_or(Ty::Unit), state.ty, value.unwrap_or(id));
                    }
                    if let Some(state) = self.loops.get_mut(target) {
                        state.broken = true;
                    }
                }
                Ty::Never
            }
            Expr::Continue { .. } => Ty::Never,
            Expr::Return { value } => {
                let ty = match value {
                    Some(value) => self.infer_expr(*value),
                    None => Ty::Unit,
                };
                self.coerce(ty, self.ret, value.unwrap_or(id));
                Ty::Never
            }
            Expr::Call { callee, args } => self.infer_call(id, *callee, args),
            Expr::Unary { op, operand } => {
                let ty = self.infer_expr(*operand);
                if *op == UnaryOp::Deref {
                    self.error(self.range(id), "dereferencing is not supported yet");
                    return Ty::Error;
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
                if bare_literal && matches!(to, Ty::Int(_)) {
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
                            diverges |= self.shallow(ty) == Ty::Never;
                            declared
                        }
                        (None, Some(init)) => {
                            let ty = self.infer_expr(init);
                            diverges |= self.shallow(ty) == Ty::Never;
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
                        self.coerce(ty, Ty::Unit, expr);
                    }
                    diverges |= self.shallow(ty) == Ty::Never;
                }
            }
        }
        match tail {
            Some(tail) => self.infer_expr(tail),
            None if diverges => Ty::Never,
            None => Ty::Unit,
        }
    }

    /// The type of an `if` with both branches: the branches' common type,
    /// where a `!` branch takes the other's type.
    fn join(&mut self, then_ty: Ty, else_ty: Ty, expr: ExprId) -> Ty {
        match (self.shallow(then_ty), self.shallow(else_ty)) {
            (Ty::Never, _) => else_ty,
            (_, Ty::Never) => then_ty,
            _ if self.unify(then_ty, else_ty) => then_ty,
            _ => {
                let (a, b) = (self.describe(then_ty), self.describe(else_ty));
                self.error(
                    self.range(expr),
                    format!("`if` and `else` have incompatible types: `{a}` and `{b}`"),
                );
                Ty::Error
            }
        }
    }

    fn infer_call(&mut self, id: ExprId, callee: ExprId, args: &[ExprId]) -> Ty {
        let function = match self.body.exprs[callee] {
            Expr::Path(Res::Function(function)) => function,
            Expr::Path(Res::Err) | Expr::Missing => {
                for &arg in args {
                    self.infer_expr(arg);
                }
                return Ty::Error;
            }
            _ => {
                let ty = self.infer_expr(callee);
                let ty = self.describe(ty);
                self.error(
                    self.range(callee),
                    format!("expected function, found `{ty}`"),
                );
                for &arg in args {
                    self.infer_expr(arg);
                }
                return Ty::Error;
            }
        };
        let sig = &self.sigs[function];
        let name = function_name(self.items, function);
        if args.len() != sig.params.len() {
            let (want, got) = (sig.params.len(), args.len());
            let plural = |n: usize| if n == 1 { "argument" } else { "arguments" };
            let verb = if got == 1 { "was" } else { "were" };
            self.error(
                self.range(id),
                format!(
                    "`{name}` takes {want} {} but {got} {} {verb} supplied",
                    plural(want),
                    plural(got)
                ),
            );
        }
        for (i, &arg) in args.iter().enumerate() {
            let ty = self.infer_expr(arg);
            if let Some(&param) = sig.params.get(i) {
                self.coerce(ty, param, arg);
            }
        }
        if self.items.functions[function].is_unsafe && self.unsafe_depth == 0 {
            self.error(
                self.range(id),
                format!("call to unsafe function `{name}` requires an `unsafe` block"),
            );
        }
        sig.ret
    }

    fn infer_binary(&mut self, id: ExprId, op: BinaryOp, lhs: ExprId, rhs: ExprId) -> Ty {
        match op {
            BinaryOp::Logic(_) => {
                for operand in [lhs, rhs] {
                    let ty = self.infer_expr(operand);
                    self.coerce(ty, Ty::Bool, operand);
                }
                Ty::Bool
            }
            BinaryOp::Cmp(_) => {
                let ty = self.infer_expr(lhs);
                let rhs_ty = self.infer_expr(rhs);
                self.coerce(rhs_ty, ty, rhs);
                self.deferred.push(Deferred::Binary { expr: id, op, ty });
                Ty::Bool
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
                Ty::Unit
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

    /// `ty` with every variable replaced: integer variables default to
    /// `i32`, diverging ones to `!`, and any other unknown variable is
    /// reported once and becomes an error.
    fn resolve(&mut self, ty: Ty, reported: &mut HashSet<InferVar>) -> Ty {
        let Ty::Infer(var) = self.shallow(ty) else {
            return self.shallow(ty);
        };
        let (value, origin) = self.vars[var.0 as usize];
        let ty = match value {
            VarValue::Unknown { int: true, .. } => Ty::Int(IntTy::I32),
            VarValue::Unknown {
                diverging: true, ..
            } => Ty::Never,
            _ => {
                if reported.insert(var) {
                    self.error(origin, "type annotations needed");
                }
                Ty::Error
            }
        };
        self.vars[var.0 as usize].0 = VarValue::Known(ty);
        ty
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
                    let Some(&Ty::Int(int)) = self.expr_types.get(expr) else {
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
                    let ok = match (op, ty) {
                        (_, Ty::Error | Ty::Never) => true,
                        (UnaryOp::Neg, Ty::Int(int)) => int.is_signed(),
                        (UnaryOp::Not, Ty::Int(_) | Ty::Bool) => true,
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
                    let ok = match (op, ty) {
                        (_, Ty::Error | Ty::Never) => true,
                        (BinaryOp::Cmp(_), Ty::Int(_) | Ty::Bool | Ty::Unit) => true,
                        (BinaryOp::Arith(op) | BinaryOp::Assign(Some(op)), Ty::Bool) => {
                            matches!(op, ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor)
                        }
                        (_, Ty::Int(_)) => true,
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
                    if !matches!(ty, Ty::Int(_) | Ty::Error | Ty::Never) {
                        self.error(
                            self.range(expr),
                            format!("cannot shift by a value of type `{ty}`"),
                        );
                    }
                }
                Deferred::Cast { expr, from, to } => {
                    let from = self.resolve(from, reported);
                    let ok = match (from, to) {
                        (Ty::Error | Ty::Never, _) | (_, Ty::Error) => true,
                        (Ty::Int(_) | Ty::Bool, Ty::Int(_)) => true,
                        _ => from == to,
                    };
                    if !ok {
                        let message = if to == Ty::Bool && matches!(from, Ty::Int(_)) {
                            format!("cannot cast `{from}` as `bool`")
                        } else {
                            format!("non-primitive cast: `{from}` as `{to}`")
                        };
                        self.error(self.range(expr), message);
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
