//! Type inference for one body.

use std::collections::HashSet;

use flux_arena::ArenaMap;
use flux_hir::{
    ArithOp, BinaryOp, Binding, BindingId, Body, BodySourceMap, CmpOp, Diagnostic, Expr, ExprId,
    FunctionId, ItemTree, Literal, LogicOp, LoopSource, Pat, PatId, Res, Stmt, StructId,
    StructKind, TypeRefId, UnaryOp,
};
use flux_intern::Symbol;
use flux_syntax::TextRange;

use crate::ty::{InferVar, IntTy, Mutability, Ty, TyKind};
use crate::{Signatures, function_name, resolve_or_report, self_ty};

/// The types of one body.
#[derive(Debug)]
pub struct InferenceResult {
    /// The type of every expression, except the callee of a direct call
    /// (`f` in `f(x)`), which has no value of its own.
    pub expr_types: ArenaMap<Expr, Ty>,
    pub binding_types: ArenaMap<Binding, Ty>,
    /// The field each `Field` expression reads, by index.
    pub field_indices: ArenaMap<Expr, u32>,
    /// How many references a `Field` expression's base is dereferenced
    /// through first: one for `r.a` with `r: &S`.
    pub field_derefs: ArenaMap<Expr, u32>,
    /// The method each `MethodCall` calls, and how its receiver is passed.
    pub methods: ArenaMap<Expr, Method>,
    /// Expressions of type `&mut T` that are reborrowed where they are
    /// used, rather than moved: `&*r` or `&mut *r` with this mutability.
    /// rustc does this where a reference is expected.
    pub reborrows: ArenaMap<Expr, Mutability>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A resolved method call: the receiver is dereferenced `derefs` times,
/// then borrowed (`autoref`) or passed by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Method {
    pub function: FunctionId,
    pub derefs: u32,
    pub autoref: Option<Mutability>,
}

/// Infers the types of `function`'s body.
pub fn infer_body(
    items: &ItemTree,
    sigs: &Signatures,
    function: FunctionId,
    body: &Body,
    source_map: &BodySourceMap,
) -> InferenceResult {
    let sig = &sigs[function];
    let mut ctx = Ctx {
        items,
        sigs,
        self_ty: self_ty(items, &items.functions[function]),
        body,
        source_map,
        vars: Vec::new(),
        expr_types: ArenaMap::new(),
        binding_types: ArenaMap::new(),
        field_indices: ArenaMap::new(),
        field_derefs: ArenaMap::new(),
        methods: ArenaMap::new(),
        reborrows: ArenaMap::new(),
        loops: ArenaMap::new(),
        uninit_bindings: HashSet::new(),
        unsafe_depth: u32::from(items.functions[function].is_unsafe),
        ret: sig.ret,
        deferred: Vec::new(),
        diagnostics: Vec::new(),
    };
    for (i, &pat) in body.params.iter().enumerate() {
        let ty = sig.params.get(i).copied().unwrap_or(Ty::ERROR);
        ctx.check_pat(pat, ty, false);
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
        rhs: Ty,
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
    sigs: &'a Signatures,
    /// What `Self` means in this body.
    self_ty: Option<Ty>,
    body: &'a Body,
    source_map: &'a BodySourceMap,
    /// Each variable's value and where it was created.
    vars: Vec<(VarValue, TextRange)>,
    expr_types: ArenaMap<Expr, Ty>,
    binding_types: ArenaMap<Binding, Ty>,
    field_indices: ArenaMap<Expr, u32>,
    field_derefs: ArenaMap<Expr, u32>,
    methods: ArenaMap<Expr, Method>,
    reborrows: ArenaMap<Expr, Mutability>,
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
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
                // Every element is unified, even after a mismatch, so that
                // each learns what it can.
                let mut ok = xs.len() == ys.len();
                for (&x, &y) in xs.iter().zip(ys) {
                    ok &= self.unify(x, y);
                }
                ok
            }
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
        let blamed = self.blame(expr);
        // A `&mut T` where a reference is expected is reborrowed, and may
        // become a `&T`.
        if let (TyKind::Ref(Mutability::Mut, pointee), TyKind::Ref(m, expected_pointee)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            self.reborrows.insert(expr, m);
            if !self.unify(pointee, expected_pointee) {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        if let (TyKind::Ref(..), TyKind::RawPtr(..)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            self.error(
                self.range(blamed),
                "coercing references to raw pointers is not supported yet",
            );
            return;
        }
        // A tuple expression is checked element by element, as rustc
        // does, so a mismatch is reported at the element.
        if let Expr::Tuple(fields) = &self.body.exprs[blamed]
            && let TyKind::Tuple(elements) = self.shallow(expected).kind()
        {
            if fields.len() == elements.len() {
                for (&field, &element) in fields.iter().zip(elements) {
                    let ty = self.expr_types.get(field).copied().unwrap_or(Ty::ERROR);
                    self.coerce(ty, element, field);
                }
            } else {
                let (want, got) = (elements.len(), fields.len());
                self.error(
                    self.range(blamed),
                    format!(
                        "mismatched types: expected a tuple with {want} element{}, found one with {got} element{}",
                        plural(want),
                        plural(got)
                    ),
                );
            }
            return;
        }
        if !self.unify(actual, expected) {
            let range = self.range(blamed);
            self.mismatch(expected, actual, range);
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
            TyKind::Tuple(elements) if !elements.is_empty() => {
                let elements: Vec<String> = elements.iter().map(|&e| self.describe(e)).collect();
                if elements.len() == 1 {
                    format!("({},)", elements[0])
                } else {
                    format!("({})", elements.join(", "))
                }
            }
            _ => ty.to_string(),
        }
    }

    fn mismatch(&mut self, expected: Ty, actual: Ty, range: TextRange) {
        let (expected, actual) = (self.describe(expected), self.describe(actual));
        self.error(
            range,
            format!("mismatched types: expected `{expected}`, found `{actual}`"),
        );
    }

    fn resolve_type_ref(&mut self, ty: TypeRefId) -> Ty {
        let range = self.source_map.types.get(ty).copied().unwrap_or_default();
        resolve_or_report(
            self.items,
            &self.body.types[ty],
            self.self_ty,
            range,
            &mut self.diagnostics,
        )
    }

    fn struct_ty(&self, strukt: StructId) -> Ty {
        Ty::adt(strukt, self.items.structs[strukt].name)
    }

    // Patterns.

    /// Checks `pat` against a value of type `expected` and gives its
    /// bindings their types; `uninit` for a `let` without a value.
    fn check_pat(&mut self, pat: PatId, expected: Ty, uninit: bool) {
        let (body, items, sigs) = (self.body, self.items, self.sigs);
        let range = self.source_map.pats.get(pat).copied().unwrap_or_default();
        match &body.pats[pat] {
            Pat::Missing | Pat::Wild => {}
            Pat::Bind(binding) => {
                self.binding_types.insert(*binding, expected);
                if uninit {
                    self.uninit_bindings.insert(*binding);
                }
            }
            Pat::Tuple { pats, rest } => {
                let elements = match self.shallow(expected).kind() {
                    TyKind::Tuple(elements) => {
                        let fits = match rest {
                            Some(_) => pats.len() <= elements.len(),
                            None => pats.len() == elements.len(),
                        };
                        if !fits {
                            let (want, got) = (elements.len(), pats.len());
                            self.error(
                                range,
                                format!(
                                    "mismatched types: expected a tuple with {want} element{}, found one with {got} element{}",
                                    plural(want),
                                    plural(got)
                                ),
                            );
                        }
                        fits.then_some(elements)
                    }
                    TyKind::Error => None,
                    TyKind::Infer(_) if rest.is_none() => {
                        let vars: Vec<Ty> = pats
                            .iter()
                            .map(|&p| {
                                let range = self.source_map.pats.get(p).copied();
                                self.new_var(false, range.unwrap_or_default())
                            })
                            .collect();
                        let tuple = Ty::tuple(&vars);
                        self.unify(expected, tuple);
                        Some(tuple_elements(tuple))
                    }
                    TyKind::Infer(_) => {
                        self.error(range, "type annotations needed");
                        None
                    }
                    _ => {
                        let found: Vec<&str> = pats.iter().map(|_| "_").collect();
                        let found = match found.len() {
                            1 => "(_,)".to_owned(),
                            _ => format!("({})", found.join(", ")),
                        };
                        let expected = self.describe(expected);
                        self.error(
                            range,
                            format!("mismatched types: expected `{expected}`, found `{found}`"),
                        );
                        None
                    }
                };
                self.check_tuple_pats(pats, *rest, elements, uninit);
            }
            Pat::TupleStruct { strukt, pats, rest } => {
                let ty = self.struct_ty(*strukt);
                let fields = &sigs.structs[*strukt];
                if !self.unify(ty, expected) {
                    self.mismatch(expected, ty, range);
                }
                let fits = match rest {
                    Some(_) => pats.len() <= fields.len(),
                    None => pats.len() == fields.len(),
                };
                if !fits {
                    let (want, got) = (fields.len(), pats.len());
                    // rustc points at the fields, not the whole pattern.
                    let range = pats
                        .first()
                        .and_then(|&p| self.source_map.pats.get(p).copied())
                        .unwrap_or(range);
                    self.error(
                        range,
                        format!(
                            "this pattern has {got} field{}, but the corresponding tuple struct has {want} field{}",
                            plural(got),
                            plural(want)
                        ),
                    );
                }
                self.check_tuple_pats(pats, *rest, fits.then_some(fields), uninit);
            }
            Pat::Record {
                strukt,
                fields,
                rest,
            } => {
                let ty = self.struct_ty(*strukt);
                if !self.unify(ty, expected) {
                    self.mismatch(expected, ty, range);
                }
                let def = &items.structs[*strukt];
                let names = self.source_map.pat_fields.get(pat);
                let mut mentioned = Vec::new();
                for (i, &(name, field_pat)) in fields.iter().enumerate() {
                    let (name_range, field_range) = names
                        .and_then(|n| n.get(i))
                        .copied()
                        .unwrap_or((range, range));
                    let field_ty = match def.field(name) {
                        _ if mentioned.contains(&name) => {
                            self.error(
                                field_range,
                                format!("field `{name}` bound multiple times in the pattern"),
                            );
                            Ty::ERROR
                        }
                        Some((index, _)) => sigs.structs[*strukt][index],
                        None => {
                            let message = format!(
                                "struct `{}` does not have a field named `{name}`",
                                def.name
                            );
                            self.error(name_range, message);
                            Ty::ERROR
                        }
                    };
                    mentioned.push(name);
                    self.check_pat(field_pat, field_ty, uninit);
                }
                if !rest {
                    let missing: Vec<Symbol> = def
                        .fields
                        .iter()
                        .map(|f| f.name)
                        .filter(|name| !mentioned.contains(name))
                        .collect();
                    if !missing.is_empty() {
                        // Unlike struct expressions, every field is listed.
                        let names: Vec<String> = missing.iter().map(|n| format!("`{n}`")).collect();
                        let message = format!(
                            "pattern does not mention field{} {}",
                            plural(names.len()),
                            names.join(", ")
                        );
                        self.error(range, message);
                    }
                }
            }
            Pat::Unit(strukt) => {
                let ty = self.struct_ty(*strukt);
                if !self.unify(ty, expected) {
                    self.mismatch(expected, ty, range);
                }
            }
        }
    }

    /// The fields of a tuple or tuple struct pattern against the types of
    /// `elements`, if they fit.
    fn check_tuple_pats(
        &mut self,
        pats: &[PatId],
        rest: Option<u32>,
        elements: Option<&[Ty]>,
        uninit: bool,
    ) {
        for (i, &pat) in pats.iter().enumerate() {
            let ty = match (elements, rest) {
                (Some(elements), Some(rest)) if i >= rest as usize => {
                    elements[elements.len() - (pats.len() - i)]
                }
                (Some(elements), _) => elements[i],
                (None, _) => Ty::ERROR,
            };
            self.check_pat(pat, ty, uninit);
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
            Expr::Path(Res::Struct(strukt)) => match self.items.structs[*strukt].kind {
                StructKind::Unit => self.struct_ty(*strukt),
                _ => {
                    self.error(self.range(id), "functions as values are not supported yet");
                    Ty::ERROR
                }
            },
            Expr::Path(Res::Err) => Ty::ERROR,
            Expr::Tuple(fields) => {
                let elements: Vec<Ty> = fields.iter().map(|&f| self.infer_expr(f)).collect();
                Ty::tuple(&elements)
            }
            Expr::Field { expr, name } => self.infer_field(id, *expr, *name),
            Expr::Record {
                strukt,
                fields,
                spread,
            } => self.infer_record(id, *strukt, fields, *spread),
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
            Expr::Ref { mutable, expr } => {
                let ty = self.infer_expr(*expr);
                let m = if *mutable {
                    Mutability::Mut
                } else {
                    Mutability::Not
                };
                Ty::new(TyKind::Ref(m, ty))
            }
            Expr::Unary { op, operand } => {
                let ty = self.infer_expr(*operand);
                if *op == UnaryOp::Deref {
                    return self.infer_deref(id, ty);
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
                    self.check_pat(pat, ty, init.is_none());
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
            Expr::Path(Res::Struct(strukt)) => {
                return self.infer_constructor(callee, strukt, args);
            }
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
            let verb = if got == 1 { "was" } else { "were" };
            self.error(
                self.range(callee),
                format!(
                    "this function takes {at_least}{want} argument{} but {got} argument{} {verb} supplied",
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

    /// `P(a, b)`, a tuple struct's constructor.
    fn infer_constructor(&mut self, callee: ExprId, strukt: StructId, args: &[ExprId]) -> Ty {
        let def = &self.items.structs[strukt];
        let sigs = self.sigs;
        if def.kind != StructKind::Tuple {
            let name = def.name;
            self.error(
                self.range(callee),
                format!("expected function, found struct `{name}`"),
            );
            self.infer_args(args);
            return Ty::ERROR;
        }
        let fields = &sigs.structs[strukt];
        if fields.len() != args.len() {
            let (want, got) = (fields.len(), args.len());
            let verb = if got == 1 { "was" } else { "were" };
            self.error(
                self.range(callee),
                format!(
                    "this struct takes {want} argument{} but {got} argument{} {verb} supplied",
                    plural(want),
                    plural(got)
                ),
            );
        }
        for (i, &arg) in args.iter().enumerate() {
            let ty = self.infer_expr(arg);
            if let Some(&field) = fields.get(i) {
                self.coerce(ty, field, arg);
            }
        }
        self.struct_ty(strukt)
    }

    /// `base.name`: a struct's field or a tuple's element.
    /// `*expr` for an `expr` of type `ty`.
    fn infer_deref(&mut self, id: ExprId, ty: Ty) -> Ty {
        match self.shallow(ty).kind() {
            TyKind::Ref(_, pointee) => pointee,
            TyKind::RawPtr(_, pointee) => {
                if self.unsafe_depth == 0 {
                    self.error(
                        self.range(id),
                        "dereference of raw pointer is unsafe and requires unsafe block",
                    );
                }
                pointee
            }
            TyKind::Error => Ty::ERROR,
            TyKind::Infer(var) if !self.unknown(var).0 => {
                self.error(self.range(id), "type annotations needed");
                Ty::ERROR
            }
            _ => {
                let ty = self.describe(ty);
                self.error(
                    self.range(id),
                    format!("type `{ty}` cannot be dereferenced"),
                );
                Ty::ERROR
            }
        }
    }

    /// `ty` with references removed, and how many there were.
    fn autoderef(&mut self, ty: Ty) -> (Ty, u32) {
        let mut ty = self.shallow(ty);
        let mut derefs = 0;
        while let TyKind::Ref(_, pointee) = ty.kind() {
            ty = self.shallow(pointee);
            derefs += 1;
        }
        (ty, derefs)
    }

    fn infer_field(&mut self, id: ExprId, base: ExprId, name: Symbol) -> Ty {
        let ty = self.infer_expr(base);
        let (base_ty, derefs) = self.autoderef(ty);
        if derefs > 0 {
            self.field_derefs.insert(id, derefs);
        }
        let (items, sigs) = (self.items, self.sigs);
        let range = self
            .source_map
            .expr_fields
            .get(id)
            .and_then(|names| names.first())
            .copied()
            .unwrap_or_else(|| self.range(id));
        let found = match base_ty.kind() {
            TyKind::Tuple(elements) => name
                .as_str()
                .parse::<usize>()
                .ok()
                .filter(|i| i.to_string() == name.as_str())
                .and_then(|i| Some((i, *elements.get(i)?))),
            TyKind::Adt(adt) => items.structs[adt.strukt]
                .field(name)
                .map(|(i, _)| (i, sigs.structs[adt.strukt][i])),
            TyKind::Error => return Ty::ERROR,
            TyKind::Int(_) | TyKind::Bool => {
                self.error(
                    range,
                    format!("`{base_ty}` is a primitive type and therefore doesn't have fields"),
                );
                return Ty::ERROR;
            }
            TyKind::Infer(var) if self.unknown(var).0 => {
                self.error(
                    range,
                    "`{integer}` is a primitive type and therefore doesn't have fields",
                );
                return Ty::ERROR;
            }
            TyKind::Infer(_) => {
                self.error(self.range(base), "type annotations needed");
                return Ty::ERROR;
            }
            _ => None,
        };
        match found {
            Some((index, ty)) => {
                self.field_indices.insert(id, index as u32);
                ty
            }
            None => {
                // rustc names the type before dereferencing: `&S`.
                let is_method = match base_ty.kind() {
                    TyKind::Adt(adt) => items
                        .assoc_function(adt.strukt, name)
                        .is_some_and(|f| items.functions[f].has_self),
                    _ => false,
                };
                let ty = self.describe(ty);
                let message = if is_method {
                    format!("attempted to take value of method `{name}` on type `{ty}`")
                } else {
                    format!("no field `{name}` on type `{ty}`")
                };
                self.error(range, message);
                Ty::ERROR
            }
        }
    }

    /// `S { a: 1, b, ..base }`
    fn infer_record(
        &mut self,
        id: ExprId,
        strukt: StructId,
        fields: &[(Symbol, ExprId)],
        spread: Option<ExprId>,
    ) -> Ty {
        let (items, sigs) = (self.items, self.sigs);
        let def = &items.structs[strukt];
        let ty = self.struct_ty(strukt);
        let range = self.range(id);
        let names = self.source_map.expr_fields.get(id);
        let mut seen = Vec::new();
        let mut unknown = false;
        for (i, &(name, value)) in fields.iter().enumerate() {
            let value_ty = self.infer_expr(value);
            let name_range = names.and_then(|n| n.get(i)).copied().unwrap_or(range);
            if seen.contains(&name) {
                self.error(
                    name_range,
                    format!("field `{name}` specified more than once"),
                );
                continue;
            }
            seen.push(name);
            match def.field(name) {
                Some((index, _)) => self.coerce(value_ty, sigs.structs[strukt][index], value),
                None => {
                    unknown = true;
                    self.error(
                        name_range,
                        format!("struct `{}` has no field named `{name}`", def.name),
                    );
                }
            }
        }
        match spread {
            Some(base) => {
                let base_ty = self.infer_expr(base);
                self.coerce(base_ty, ty, base);
            }
            None => {
                let missing: Vec<Symbol> = def
                    .fields
                    .iter()
                    .map(|f| f.name)
                    .filter(|name| !seen.contains(name))
                    .collect();
                // rustc reports unknown fields instead of missing ones.
                if !missing.is_empty() && !unknown {
                    let message = format!(
                        "missing {} in initializer of `{}`",
                        list_fields(&missing),
                        def.name
                    );
                    let path = self.source_map.paths.get(id).copied();
                    self.error(path.unwrap_or(range), message);
                }
            }
        }
        ty
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
        let name_range = self
            .source_map
            .expr_fields
            .get(id)
            .and_then(|names| names.first())
            .copied()
            .unwrap_or_else(|| self.range(id));
        let (base, derefs) = self.autoderef(receiver_ty);
        if let TyKind::Adt(adt) = base.kind() {
            let found = self.items.assoc_function(adt.strukt, method);
            let data = found.map(|f| &self.items.functions[f]);
            match (found, data) {
                (Some(function), Some(data)) if data.has_self => {
                    return self.check_method(id, function, derefs, args, name_range);
                }
                _ => {
                    let what = self.describe(receiver_ty);
                    let kind = if derefs > 0 { "reference" } else { "struct" };
                    self.error(
                        name_range,
                        format!("no method named `{method}` found for {kind} `{what}` in the current scope"),
                    );
                    return Ty::ERROR;
                }
            }
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
        let message = match receiver_ty.kind() {
            TyKind::Int(_) | TyKind::Bool | TyKind::Tuple(_) | TyKind::RawPtr(..) => {
                format!("methods on `{ty}` are not supported yet")
            }
            _ => format!("the method `{method}` on `{ty}` is not supported yet"),
        };
        self.error(name_range, message);
        Ty::ERROR
    }

    /// A call of `function`, a method, on a receiver with `derefs`
    /// references taken off it.
    fn check_method(
        &mut self,
        id: ExprId,
        function: FunctionId,
        derefs: u32,
        args: &[ExprId],
        name_range: TextRange,
    ) -> Ty {
        let sig = &self.sigs[function];
        let autoref = match sig.params[0].kind() {
            TyKind::Ref(m, _) => Some(m),
            _ => None,
        };
        self.methods.insert(
            id,
            Method {
                function,
                derefs,
                autoref,
            },
        );
        let params = &sig.params[1..];
        if params.len() != args.len() {
            let (want, got) = (params.len(), args.len());
            let verb = if got == 1 { "was" } else { "were" };
            self.error(
                name_range,
                format!(
                    "this method takes {want} argument{} but {got} argument{} {verb} supplied",
                    plural(want),
                    plural(got)
                ),
            );
        }
        for (&arg, &param) in args.iter().zip(params) {
            let ty = self.expr_types.get(arg).copied().unwrap_or(Ty::ERROR);
            self.coerce(ty, param, arg);
        }
        if self.items.functions[function].is_unsafe && self.unsafe_depth == 0 {
            let name = self.items.function_path(&self.items.functions[function]);
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
                    self.coerce(ty, Ty::BOOL, operand);
                }
                Ty::BOOL
            }
            BinaryOp::Cmp(_) => {
                let ty = self.infer_expr(lhs);
                let rhs_ty = self.infer_expr(rhs);
                if self.is_primitive_operand(ty) {
                    self.coerce(rhs_ty, ty, rhs);
                }
                self.deferred.push(Deferred::Binary {
                    expr: id,
                    op,
                    ty,
                    rhs: rhs_ty,
                });
                Ty::BOOL
            }
            BinaryOp::Arith(arith) => self.infer_arith(id, op, arith, lhs, rhs),
            BinaryOp::Assign(arith) => {
                self.check_place(id, lhs, arith.is_none());
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

    /// Whether the operators on a left operand of type `ty` are the
    /// built-in ones, whose right operand has the same type. Other types'
    /// operators come from traits, which say what the right operand is.
    fn is_primitive_operand(&mut self, ty: Ty) -> bool {
        !matches!(
            self.shallow(ty).kind(),
            TyKind::Ref(..) | TyKind::Adt(_) | TyKind::Tuple([_, ..]) | TyKind::RawPtr(..)
        )
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
        } else if self.is_primitive_operand(ty) {
            self.coerce(rhs_ty, ty, rhs);
        } else {
            // A trait would give the result's type; the deferred check
            // reports the operator.
            self.deferred.push(Deferred::Binary {
                expr: id,
                op,
                ty,
                rhs: rhs_ty,
            });
            return Ty::ERROR;
        }
        self.deferred.push(Deferred::Binary {
            expr: id,
            op,
            ty,
            rhs: rhs_ty,
        });
        ty
    }

    /// The left side of an assignment must be a local or a field of one;
    /// a local without `mut` only if it was declared without a value
    /// (`let x; x = 1;`). Assignments to fields are checked with
    /// initialization, in MIR.
    fn check_place(&mut self, assignment: ExprId, lhs: ExprId, plain: bool) {
        match self.body.exprs[lhs] {
            Expr::Field { .. }
            | Expr::Unary {
                op: UnaryOp::Deref, ..
            } => {
                // A field of a local, or anything behind a pointer.
                let mut base = lhs;
                loop {
                    match self.body.exprs[base] {
                        Expr::Field { expr, .. } => base = expr,
                        Expr::Unary {
                            op: UnaryOp::Deref, ..
                        } => return,
                        _ => break,
                    }
                }
                if !matches!(
                    self.body.exprs[base],
                    Expr::Path(Res::Local(_) | Res::Err) | Expr::Missing
                ) {
                    self.error(
                        self.range(lhs),
                        "assigning to fields of temporaries is not supported yet",
                    );
                }
            }
            Expr::Path(Res::Local(binding)) => {
                let data = &self.body.bindings[binding];
                let first_assignment = plain && self.uninit_bindings.contains(&binding);
                if !data.mutable && !first_assignment {
                    let name = data.name;
                    let is_param = self
                        .body
                        .params
                        .iter()
                        .any(|&p| self.body.pats[p] == Pat::Bind(binding));
                    let message = if is_param {
                        format!("cannot assign to immutable argument `{name}`")
                    } else {
                        format!("cannot assign twice to immutable variable `{name}`")
                    };
                    self.error(self.range(assignment), message);
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
        self.check_references();
        InferenceResult {
            expr_types: self.expr_types,
            binding_types: self.binding_types,
            field_indices: self.field_indices,
            field_derefs: self.field_derefs,
            methods: self.methods,
            reborrows: self.reborrows,
            diagnostics: self.diagnostics,
        }
    }

    /// Reports references where flux cannot check them yet. Without
    /// lifetimes, a reference can be checked only while each one lives in
    /// a single local that is assigned once: not in a `mut` binding, a
    /// binding declared without a value, another type, or a `break`
    /// value.
    fn check_references(&mut self) {
        let body = self.body;
        for (pat, data) in body.pats.iter() {
            let Pat::Bind(binding) = data else {
                continue;
            };
            let ty = self
                .binding_types
                .get(*binding)
                .copied()
                .unwrap_or(Ty::ERROR);
            if !crate::contains_ref(ty) {
                continue;
            }
            let range = self.source_map.pats.get(pat).copied().unwrap_or_default();
            if body.bindings[*binding].mutable {
                self.error(range, "`mut` bindings of references are not supported yet");
            } else if self.uninit_bindings.contains(binding) {
                self.error(
                    range,
                    "references in variables declared without a value are not supported yet",
                );
            }
        }
        let mut reported = false;
        for (expr, data) in body.exprs.iter() {
            let ty = self.expr_types.get(expr).copied().unwrap_or(Ty::ERROR);
            let nested = match ty.kind() {
                TyKind::Ref(_, pointee) => crate::contains_ref(pointee),
                _ => crate::contains_ref(ty),
            };
            if nested && !reported {
                reported = true;
                self.error(
                    self.range(expr),
                    "references inside other types are not supported yet",
                );
            }
            if let Expr::Break {
                value: Some(value), ..
            } = data
            {
                let ty = self.expr_types.get(*value).copied().unwrap_or(Ty::ERROR);
                if crate::contains_ref(ty) {
                    self.error(
                        self.range(*value),
                        "`break` with a reference is not supported yet",
                    );
                }
            }
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
            TyKind::Tuple(elements) => {
                let elements: Vec<Ty> = elements
                    .iter()
                    .map(|&e| self.resolve(e, reported))
                    .collect();
                Ty::tuple(&elements)
            }
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
                Deferred::Binary { expr, op, ty, rhs } => {
                    let ty = self.resolve(ty, reported);
                    let rhs = self.resolve(rhs, reported);
                    let ok = match (op, ty.kind()) {
                        (_, TyKind::Error | TyKind::Never) => true,
                        (BinaryOp::Cmp(_), TyKind::Int(_) | TyKind::Bool | TyKind::Tuple([])) => {
                            true
                        }
                        (BinaryOp::Cmp(_), TyKind::Tuple(_)) => {
                            self.error(self.range(expr), "comparing tuples is not supported yet");
                            true
                        }
                        (BinaryOp::Cmp(_), TyKind::RawPtr(..)) => {
                            self.error(self.range(expr), "comparing pointers is not supported yet");
                            true
                        }
                        (BinaryOp::Arith(op) | BinaryOp::Assign(Some(op)), TyKind::Bool) => {
                            matches!(op, ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor)
                        }
                        (_, TyKind::Int(_)) => true,
                        _ => false,
                    };
                    if !ok {
                        // rustc points at the operator, except for compound
                        // assignments.
                        let range = match op {
                            BinaryOp::Assign(_) => self.range(expr),
                            _ => self.source_map.ops.get(expr).copied().unwrap_or_default(),
                        };
                        // `&i32 + i32` and `&a == &b` are valid, through
                        // trait implementations.
                        let message = if ty.is_pointer() || rhs.is_pointer() {
                            "operators on references are not supported yet".to_owned()
                        } else {
                            binary_error(op, ty, rhs)
                        };
                        self.error(range, message);
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
                    if matches!(ty.kind(), TyKind::Adt(_) | TyKind::Tuple([_, ..])) {
                        self.error(
                            self.range(expr),
                            format!("passing `{ty}` to a variadic function is not supported yet"),
                        );
                    } else if let Some(cast_to) = cast_to {
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

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The elements of a tuple type.
fn tuple_elements(ty: Ty) -> &'static [Ty] {
    match ty.kind() {
        TyKind::Tuple(elements) => elements,
        _ => &[],
    }
}

/// Field names as rustc lists them: "field `a`", "fields `a` and `b`",
/// "fields `a`, `b` and `c`", "fields `a`, `b`, `c` and 2 other fields".
fn list_fields(names: &[Symbol]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
    match quoted.as_slice() {
        [only] => format!("field {only}"),
        [init @ .., last] if quoted.len() <= 3 => format!("fields {} and {last}", init.join(", ")),
        _ => {
            let others = quoted.len() - 3;
            format!(
                "fields {} and {others} other field{}",
                quoted[..3].join(", "),
                plural(others)
            )
        }
    }
}

/// rustc's message for an operator its operand types do not support.
fn binary_error(op: BinaryOp, lhs: Ty, rhs: Ty) -> String {
    let op_str = op_text(op);
    match op {
        BinaryOp::Arith(ArithOp::Add) => format!("cannot add `{rhs}` to `{lhs}`"),
        BinaryOp::Arith(ArithOp::Sub) => format!("cannot subtract `{rhs}` from `{lhs}`"),
        BinaryOp::Arith(ArithOp::Mul) => format!("cannot multiply `{lhs}` by `{rhs}`"),
        BinaryOp::Arith(ArithOp::Div) => format!("cannot divide `{lhs}` by `{rhs}`"),
        BinaryOp::Arith(ArithOp::Rem) => {
            format!("cannot calculate the remainder of `{lhs}` divided by `{rhs}`")
        }
        BinaryOp::Arith(_) => format!("no implementation for `{lhs} {op_str} {rhs}`"),
        BinaryOp::Assign(_) => {
            format!("binary assignment operation `{op_str}` cannot be applied to type `{lhs}`")
        }
        BinaryOp::Cmp(_) | BinaryOp::Logic(_) => {
            format!("binary operation `{op_str}` cannot be applied to type `{lhs}`")
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
