//! Type inference for one body.

mod solve;

use std::collections::HashSet;

use flux_arena::ArenaMap;
use flux_hir::{
    ArithOp, BinaryOp, Binding, BindingId, Body, BodySourceMap, CmpOp, ConstId, Diagnostic, Expr,
    ExprId, FunctionId, Generics, ItemTree, Literal, LogicOp, LoopSource, Pat, PatId, Res,
    SegmentArgs, Stmt, StructId, StructKind, TypeRefId, UnaryOp,
};
use flux_intern::Symbol;
use flux_syntax::TextRange;

use crate::traits::{Predicate, elaborate, predicates_of};
use crate::ty::{InferVar, IntTy, Mutability, Ty, TyKind};
use crate::{
    FnSig, Instance, Signatures, TypeScope, function_name, generic_names, impl_target,
    resolve_or_report, self_ty, wrong_arg_count,
};
use solve::{CallSite, Obligation};

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
    /// The function each call calls, with its generic arguments: at the
    /// callee `Path`, or the `MethodCall`. Not always the path's: of the
    /// associated functions of a generic struct with one name, the one
    /// for the type it is called on.
    pub calls: ArenaMap<Expr, Instance>,
    /// The trait method each operator on a type other than the integers
    /// and `bool` calls: `Add::add` for `a + b`, `PartialEq::eq` for
    /// `a == b`, `AddAssign::add_assign` for `a += b`, `Neg::neg` for
    /// `-a`.
    pub overloads: ArenaMap<Expr, Instance>,
    /// Comparisons of references, `&a == &b`, whose operands are passed
    /// as they are: `PartialEq::eq::<A, B>(a, b)`, as `core`'s impl for
    /// references would.
    pub compared_refs: ArenaMap<Expr, ()>,
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
    let data = &items.functions[function];
    let owner = Owner {
        module: data.loc.module,
        self_ty: self_ty(sigs, data.owner),
        generics: generic_names(items, data.owner, &data.generics),
        is_unsafe: data.is_unsafe,
        const_context: data.is_const.then_some("constant functions"),
        ret: sig.ret,
        param_env: elaborate(sigs, &predicates_of(items, sigs, function)),
        env_projections: crate::traits::projections_of(items, sigs, function),
        self_trait: items
            .impl_of(function)
            .and_then(|imp| sigs.impl_traits.get(imp).copied()),
    };
    let mut ctx = Ctx::new(items, sigs, owner, body, source_map);
    let origin = data.def_range;
    let ret = ctx.normalize(sig.ret, origin);
    ctx.ret = ret;
    for (i, &pat) in body.params.iter().enumerate() {
        let ty = sig.params.get(i).copied().unwrap_or(Ty::ERROR);
        let ty = ctx.normalize(ty, origin);
        ctx.check_pat(pat, ty, false);
    }
    ctx.expect(body.root, ret);
    let ty = ctx.infer_expr(body.root);
    ctx.coerce(ty, ret, body.root);
    if let Some(flux_hir::Owner::Trait(_)) = data.owner {
        ctx.check_unsized_self();
    }
    ctx.finish()
}

/// Infers the types of constant `konst`'s value.
pub fn infer_const(
    items: &ItemTree,
    sigs: &Signatures,
    konst: ConstId,
    body: &Body,
    source_map: &BodySourceMap,
) -> InferenceResult {
    let data = &items.consts[konst];
    let ret = sigs.consts[konst];
    let owner = Owner {
        module: data.loc.module,
        self_ty: self_ty(sigs, data.owner),
        generics: generic_names(items, data.owner, &Generics::default()),
        is_unsafe: false,
        const_context: Some("constants"),
        ret,
        param_env: data
            .owner
            .and_then(flux_hir::Owner::as_impl)
            .and_then(|imp| sigs.impl_predicates.get(imp))
            .map_or(Vec::new(), |p| elaborate(sigs, p)),
        env_projections: Vec::new(),
        self_trait: None,
    };
    let mut ctx = Ctx::new(items, sigs, owner, body, source_map);
    ctx.expect(body.root, ret);
    let ty = ctx.infer_expr(body.root);
    ctx.coerce(ty, ret, body.root);
    ctx.finish()
}

/// What a body belongs to, as far as inference cares.
struct Owner {
    module: flux_hir::ModuleId,
    self_ty: Option<Ty>,
    /// The generic parameters in scope, by index.
    generics: Vec<Symbol>,
    is_unsafe: bool,
    /// "constants" or "constant functions", where only `const fn`s may be
    /// called.
    const_context: Option<&'static str>,
    ret: Ty,
    /// The bounds in scope, with the supertraits they imply.
    param_env: Vec<Predicate>,
    /// What the bounds in scope say associated types are.
    env_projections: Vec<crate::Projection>,
    /// In a trait's `impl` block, its trait.
    self_trait: Option<crate::TraitRef>,
}

impl<'a> Ctx<'a> {
    fn new(
        items: &'a ItemTree,
        sigs: &'a Signatures,
        owner: Owner,
        body: &'a Body,
        source_map: &'a BodySourceMap,
    ) -> Ctx<'a> {
        Ctx {
            items,
            sigs,
            self_ty: owner.self_ty,
            generics: owner.generics,
            module: owner.module,
            const_context: owner.const_context,
            body,
            source_map,
            vars: Vec::new(),
            expr_types: ArenaMap::new(),
            binding_types: ArenaMap::new(),
            field_indices: ArenaMap::new(),
            field_derefs: ArenaMap::new(),
            methods: ArenaMap::new(),
            reborrows: ArenaMap::new(),
            calls: ArenaMap::new(),
            overloads: ArenaMap::new(),
            compared_refs: ArenaMap::new(),
            expected: ArenaMap::new(),
            loops: ArenaMap::new(),
            reported_vars: HashSet::new(),
            uninit_bindings: HashSet::new(),
            tainted: false,
            unsafe_depth: u32::from(owner.is_unsafe),
            ret: owner.ret,
            param_env: owner.param_env,
            env_projections: owner.env_projections,
            self_trait: owner.self_trait,
            projections: Vec::new(),
            obligations: Vec::new(),
            traits_in_scope: None,
            reported_unmet: HashSet::new(),
            deferred: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
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
    /// The generic parameters in scope, by index.
    generics: Vec<Symbol>,
    /// The function's module, where the types in its body are resolved.
    module: flux_hir::ModuleId,
    /// In a constant or `const fn`: what to call it in E0015.
    const_context: Option<&'static str>,
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
    calls: ArenaMap<Expr, (FunctionId, Vec<Ty>)>,
    overloads: ArenaMap<Expr, (FunctionId, Vec<Ty>)>,
    compared_refs: ArenaMap<Expr, ()>,
    /// The type the context of an expression wants, where known before
    /// it is inferred: a call or struct expression's generic arguments
    /// follow from it, so mismatches are reported at its parts.
    expected: ArenaMap<Expr, Ty>,
    loops: ArenaMap<Expr, LoopState>,
    /// Variables already reported as needing a type.
    reported_vars: HashSet<InferVar>,
    /// `let x;` bindings, which may be assigned once without `mut`.
    uninit_bindings: HashSet<BindingId>,
    /// Has errors, or values of erroneous types, once inference is done:
    /// variables left unknown are then not reported.
    tainted: bool,
    unsafe_depth: u32,
    ret: Ty,
    /// The bounds in scope, with the supertraits they imply.
    param_env: Vec<Predicate>,
    /// What the bounds in scope say associated types are.
    env_projections: Vec<crate::Projection>,
    /// In a trait's `impl` block, its trait: `Self::Out` is its type.
    self_trait: Option<crate::TraitRef>,
    /// Associated types whose `Self` is not known yet, each standing for
    /// a variable until it is.
    projections: Vec<solve::PendingProjection>,
    /// Trait obligations not yet met.
    obligations: Vec<Obligation>,
    traits_in_scope: Option<Vec<flux_hir::TraitId>>,
    /// Unmet obligations reported, by range and message.
    reported_unmet: HashSet<(TextRange, String)>,
    deferred: Vec<Deferred>,
    diagnostics: Vec<Diagnostic>,
}

impl Ctx<'_> {
    fn range(&self, expr: ExprId) -> TextRange {
        self.source_map.exprs.get(expr).copied().unwrap_or_default()
    }

    fn error(&mut self, range: TextRange, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            file: flux_hir::FileId::UNKNOWN,
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
            (TyKind::Adt(a, xs), TyKind::Adt(b, ys)) if a == b => {
                let mut ok = true;
                for (&x, &y) in xs.iter().zip(ys) {
                    ok &= self.unify(x, y);
                }
                ok
            }
            (TyKind::Alias(a), TyKind::Alias(b)) if a.trait_ == b.trait_ && a.name == b.name => {
                let mut ok = true;
                for (&x, &y) in a.args.iter().zip(b.args) {
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

    /// Notes that `expr`'s context wants a value of type `ty`.
    fn expect(&mut self, expr: ExprId, ty: Ty) {
        let expr = self.blame(expr);
        self.expected.insert(expr, ty);
    }

    /// Unifies `ty`, the type of expression `expr` before its parts are
    /// inferred, with the type its context wants, if they can be.
    fn meet_expectation(&mut self, expr: ExprId, ty: Ty) {
        if let Some(&expected) = self.expected.get(expr)
            && self.can_unify(ty, expected)
        {
            self.unify(ty, expected);
        }
    }

    /// Whether `a` and `b` would unify, without unifying them.
    fn can_unify(&mut self, a: Ty, b: Ty) -> bool {
        let (a, b) = (self.shallow(a), self.shallow(b));
        if a == b {
            return true;
        }
        match (a.kind(), b.kind()) {
            (TyKind::Error, _) | (_, TyKind::Error) => true,
            (TyKind::Infer(_), TyKind::Infer(_)) => true,
            (TyKind::Infer(var), _) => !self.unknown(var).0 || b.as_int().is_some(),
            (_, TyKind::Infer(var)) => !self.unknown(var).0 || a.as_int().is_some(),
            (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y))
            | (TyKind::Ref(m, x), TyKind::Ref(n, y)) => m == n && self.can_unify(x, y),
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
                xs.len() == ys.len() && xs.iter().zip(ys).all(|(&x, &y)| self.can_unify(x, y))
            }
            (TyKind::Adt(a, xs), TyKind::Adt(b, ys)) => {
                a == b && xs.iter().zip(ys).all(|(&x, &y)| self.can_unify(x, y))
            }
            (TyKind::Alias(a), TyKind::Alias(b)) => {
                a.trait_ == b.trait_
                    && a.name == b.name
                    && a.args
                        .iter()
                        .zip(b.args)
                        .all(|(&x, &y)| self.can_unify(x, y))
            }
            _ => false,
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
            TyKind::Adt(adt, args) if !args.is_empty() => {
                let args: Vec<String> = args.iter().map(|&a| self.describe(a)).collect();
                format!("{}<{}>", adt.name, args.join(", "))
            }
            TyKind::Alias(alias) => {
                let args: Vec<String> = alias.args.iter().map(|&a| self.describe(a)).collect();
                let trait_args = if args.len() > 1 {
                    format!("<{}>", args[1..].join(", "))
                } else {
                    String::new()
                };
                format!(
                    "<{} as {}{trait_args}>::{}",
                    args[0], alias.trait_name, alias.name
                )
            }
            _ => ty.to_string(),
        }
    }

    fn mismatch(&mut self, expected: Ty, actual: Ty, range: TextRange) {
        // The unknown parts are not reported again.
        let mut unknown = Vec::new();
        self.unknown_vars(expected, &mut unknown);
        self.unknown_vars(actual, &mut unknown);
        for var in unknown {
            let root = self.root(var);
            self.reported_vars.insert(root);
        }
        let (expected, actual) = (self.describe(expected), self.describe(actual));
        self.error(
            range,
            format!("mismatched types: expected `{expected}`, found `{actual}`"),
        );
    }

    fn resolve_type_ref(&mut self, ty: TypeRefId) -> Ty {
        let range = self.source_map.types.get(ty).copied().unwrap_or_default();
        let scope = TypeScope {
            self_ty: self.self_ty,
            generics: &self.generics,
            signature_of: None,
            bounds: &self.param_env,
            self_trait: self.self_trait,
            defaults: Some(&self.sigs.trait_defaults),
        };
        let ty = resolve_or_report(
            self.items,
            self.module,
            &self.body.types[ty],
            scope,
            range,
            &mut self.diagnostics,
        );
        let ty = self.fill_placeholders(ty, range);
        if ty.has_aliases() {
            self.require_alias_traits(ty, range);
        }
        self.normalize(ty, range)
    }

    /// `ty` with each `_` in it a new variable.
    fn fill_placeholders(&mut self, ty: Ty, origin: TextRange) -> Ty {
        if !ty.has_infer() {
            return ty;
        }
        match ty.kind() {
            TyKind::Infer(_) => self.new_var(false, origin),
            TyKind::Tuple(elements) => {
                let elements: Vec<Ty> = elements
                    .iter()
                    .map(|&e| self.fill_placeholders(e, origin))
                    .collect();
                Ty::tuple(&elements)
            }
            TyKind::Adt(adt, args) => {
                let args: Vec<Ty> = args
                    .iter()
                    .map(|&a| self.fill_placeholders(a, origin))
                    .collect();
                Ty::adt(adt.strukt, adt.name, &args)
            }
            TyKind::RawPtr(m, pointee) => Ty::raw_ptr(m, self.fill_placeholders(pointee, origin)),
            TyKind::Ref(m, pointee) => {
                Ty::new(TyKind::Ref(m, self.fill_placeholders(pointee, origin)))
            }
            _ => ty,
        }
    }

    // Generic items.

    /// The generic arguments of a use of an item, a `kind` with `count`
    /// parameters of its own: those `written`, or new variables created
    /// at `origin`.
    fn own_args(
        &mut self,
        kind: &str,
        count: usize,
        written: Option<&SegmentArgs>,
        origin: TextRange,
    ) -> Vec<Ty> {
        // `f::<>` is `f`.
        let Some(written) = written.filter(|w| !w.types.is_empty()) else {
            return (0..count).map(|_| self.new_var(false, origin)).collect();
        };
        let args: Vec<Ty> = written
            .types
            .iter()
            .map(|&ty| self.resolve_type_ref(ty))
            .collect();
        if args.len() != count {
            // An unsupported argument was reported already.
            if !args.contains(&Ty::ERROR) {
                self.error(written.segment, wrong_arg_count(kind, count, args.len()));
            }
            return vec![Ty::ERROR; count];
        }
        args
    }

    /// Struct `strukt` as used by expression or pattern `at`, whose
    /// generic arguments are those written there, `Self`'s, or new
    /// variables.
    fn instantiate_struct(
        &mut self,
        strukt: StructId,
        at: Option<ExprId>,
        origin: TextRange,
    ) -> Ty {
        let def = &self.items.structs[strukt];
        let generics = at.and_then(|e| self.body.generic_args.get(e));
        if generics.is_some_and(|g| g.self_type)
            && let Some(self_ty) = self.self_ty
        {
            return self_ty;
        }
        if def.generics.is_empty() && generics.and_then(|g| g.item.as_ref()).is_none() {
            return Ty::adt(strukt, def.name, &[]);
        }
        let written = generics.and_then(|g| g.item.clone());
        let args = self.own_args("struct", def.generics.len(), written.as_ref(), origin);
        Ty::adt(strukt, def.name, &args)
    }

    /// The type whose block an associated function must be in, as the
    /// path `expr` naming it says: `S<A>` in `S::<A>::f`, `Self` in
    /// `Self::f`, or nothing.
    fn path_target(&mut self, expr: ExprId, function: FunctionId) -> Option<Ty> {
        let generics = self.body.generic_args.get(expr)?;
        if generics.self_type {
            return self.self_ty;
        }
        let written = generics.ty.clone()?;
        let owner = self.items.impl_of(function)?;
        let TyKind::Adt(adt, _) = self.sigs.impls.get(owner)?.kind() else {
            return None;
        };
        let want = self.items.structs[adt.strukt].generics.len();
        let args = self.own_args("struct", want, Some(&written), self.range(expr));
        Some(Ty::adt(adt.strukt, adt.name, &args))
    }

    /// The one of `candidates`, associated functions with one name, whose
    /// block may be for `target`: `Err` with how many are if not one.
    fn select(
        &mut self,
        candidates: impl Iterator<Item = FunctionId>,
        target: Option<Ty>,
    ) -> Result<FunctionId, usize> {
        let (items, sigs) = (self.items, self.sigs);
        let mut found = None;
        let mut count = 0;
        for f in candidates {
            let applies = match (target, items.impl_of(f)) {
                (Some(target), Some(owner)) => {
                    let impl_ty = sigs.impls.get(owner).copied().unwrap_or(Ty::ERROR);
                    self.impl_applies(impl_ty, target)
                }
                _ => true,
            };
            if applies {
                found.get_or_insert(f);
                count += 1;
            }
        }
        match found {
            Some(f) if count == 1 => Ok(f),
            _ => Err(count),
        }
    }

    /// Whether an `impl` block for `pattern`, in terms of its parameters,
    /// may be for `ty`: whether they unify, with the parameters and
    /// variables standing for any type.
    fn impl_applies(&mut self, pattern: Ty, ty: Ty) -> bool {
        let ty = self.shallow(ty);
        match (pattern.kind(), ty.kind()) {
            (TyKind::Param(_) | TyKind::Error, _) | (_, TyKind::Error) => true,
            (_, TyKind::Infer(var)) => !self.unknown(var).0 || pattern.as_int().is_some(),
            (TyKind::Adt(a, xs), TyKind::Adt(b, ys)) => {
                a == b && xs.iter().zip(ys).all(|(&x, &y)| self.impl_applies(x, y))
            }
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
                xs.len() == ys.len() && xs.iter().zip(ys).all(|(&x, &y)| self.impl_applies(x, y))
            }
            (TyKind::Ref(m, x), TyKind::Ref(n, y))
            | (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y)) => m == n && self.impl_applies(x, y),
            _ => pattern == ty,
        }
    }

    /// The generic arguments of a call of `function` at `expr`, a path
    /// or a method call, on a value of, or through a path naming,
    /// `target`, if known: the `impl` block's, which must be for
    /// `target`, then its own.
    fn instantiate_fn(
        &mut self,
        expr: ExprId,
        function: FunctionId,
        target: Option<Ty>,
        trait_args: Option<&[Ty]>,
        origin: TextRange,
    ) -> Vec<Ty> {
        let (items, sigs) = (self.items, self.sigs);
        let data = &items.functions[function];
        let mut args = Vec::new();
        if let Some(flux_hir::Owner::Trait(t)) = data.owner {
            // `Self`, then the trait's parameters.
            args.push(target.unwrap_or_else(|| self.new_var(false, origin)));
            match trait_args {
                Some(given) => args.extend_from_slice(given),
                None => {
                    let count = items.traits[t].generics.len();
                    args.extend((0..count).map(|_| self.new_var(false, origin)));
                }
            }
        }
        if let Some(owner) = items.impl_of(function) {
            let count = items.impls[owner].generics.len();
            let vars: Vec<Ty> = (0..count).map(|_| self.new_var(false, origin)).collect();
            let impl_ty = sigs
                .impls
                .get(owner)
                .copied()
                .unwrap_or(Ty::ERROR)
                .subst(&vars);
            if let Some(target) = target {
                // Checked by `applicable`.
                self.unify(impl_ty, target);
            }
            args.extend(vars);
        }
        let kind = match (data.owner, data.has_self) {
            (Some(flux_hir::Owner::Trait(_)), false) => "associated function",
            (_, true) => "method",
            (Some(_), false) => "associated function",
            (None, false) => "function",
        };
        let written = self
            .body
            .generic_args
            .get(expr)
            .and_then(|g| g.item.clone());
        let own = self.own_args(kind, data.generics.len(), written.as_ref(), origin);
        args.extend(own);
        self.calls.insert(expr, (function, args.clone()));
        args
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
                let ty = self.instantiate_struct(*strukt, None, range);
                let fields = sigs.fields(ty).unwrap_or(&[]);
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
                let ty = self.instantiate_struct(*strukt, None, range);
                if !self.unify(ty, expected) {
                    self.mismatch(expected, ty, range);
                }
                let field_types = sigs.fields(ty).unwrap_or(&[]);
                let def = &items.structs[*strukt];
                let names = self.source_map.pat_fields.get(pat);
                let mut private = Vec::new();
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
                        Some((index, _)) => field_types[index],
                        None => {
                            let message = format!(
                                "struct `{}` does not have a field named `{name}`",
                                def.name
                            );
                            self.error(name_range, message);
                            Ty::ERROR
                        }
                    };
                    if let Some((_, field)) = def.field(name)
                        && !items.is_visible(field.vis, self.module)
                        && !mentioned.contains(&name)
                    {
                        private.push((name, name_range));
                    }
                    mentioned.push(name);
                    self.check_pat(field_pat, field_ty, uninit);
                }
                self.private_fields(def.name, &private);
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
                let ty = self.instantiate_struct(*strukt, None, range);
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
            Expr::Path(Res::Assoc { ty, trait_, name }) => {
                if self.resolve_assoc_fn(id, *ty, *trait_, *name).is_some() {
                    self.error(self.range(id), "functions as values are not supported yet");
                }
                Ty::ERROR
            }
            Expr::Path(Res::Function(_)) => {
                self.error(self.range(id), "functions as values are not supported yet");
                Ty::ERROR
            }
            Expr::Path(Res::Struct(strukt)) => match self.items.structs[*strukt].kind {
                StructKind::Unit => self.instantiate_struct(*strukt, Some(id), self.range(id)),
                _ => {
                    self.error(self.range(id), "functions as values are not supported yet");
                    Ty::ERROR
                }
            },
            Expr::Path(Res::Const(c)) => self.sigs.consts.get(*c).copied().unwrap_or(Ty::ERROR),
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
                    Some(value) => {
                        self.expect(*value, self.ret);
                        self.infer_expr(*value)
                    }
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
                if let Some(ty) = self.overload_unary(id, *op, ty) {
                    return ty;
                }
                // `-&1` and `!&true` are valid, through `core`'s impls for
                // references.
                if let TyKind::Ref(_, pointee) = self.shallow(ty).kind() {
                    let pointee = self.shallow(pointee);
                    let valid = match (op, pointee.kind()) {
                        (UnaryOp::Neg, TyKind::Int(int)) => int.is_signed(),
                        (UnaryOp::Neg, TyKind::Infer(var)) => self.unknown(var).0,
                        (UnaryOp::Not, TyKind::Int(_) | TyKind::Bool) => true,
                        (UnaryOp::Not, TyKind::Infer(var)) => self.unknown(var).0,
                        _ => false,
                    };
                    if valid {
                        self.error(
                            self.range(id),
                            "operators on references are not supported yet",
                        );
                        return Ty::ERROR;
                    }
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
                            self.expect(init, declared);
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
        let found = match self.body.exprs[callee] {
            Expr::Path(Res::Function(function)) => self.select_path_fn(callee, function),
            Expr::Path(Res::Assoc { ty, trait_, name }) => {
                self.resolve_assoc_fn(callee, ty, trait_, name)
            }
            Expr::Path(Res::Struct(strukt)) => {
                return self.infer_constructor(id, callee, strukt, args);
            }
            Expr::Path(Res::Err) | Expr::Missing => None,
            _ => {
                let ty = self.infer_expr(callee);
                let ty = self.describe(ty);
                self.error(
                    self.range(callee),
                    format!("expected function, found `{ty}`"),
                );
                None
            }
        };
        let Some((function, target, trait_args)) = found else {
            self.infer_args(args);
            return Ty::ERROR;
        };
        let data = &self.items.functions[function];
        let name = function_name(self.items, function);
        let generic_args = self.instantiate_fn(
            callee,
            function,
            target,
            trait_args.as_deref(),
            self.range(callee),
        );
        let sig = &self.instance_sig(function, &generic_args);
        self.meet_expectation(id, sig.ret);
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
            if let Some(&param) = sig.params.get(i) {
                self.expect(arg, param);
            }
            let ty = self.infer_expr(arg);
            match sig.params.get(i) {
                Some(&param) => self.coerce(ty, param, arg),
                None if data.is_variadic => {
                    self.deferred.push(Deferred::VariadicArg { expr: arg, ty });
                }
                None => {}
            }
        }
        let segment = self.source_map.segments.get(callee).copied();
        let site = CallSite {
            callee: segment.unwrap_or_else(|| self.range(callee)),
            whole: self.range(id),
            receiver: None,
            args,
            qself: self.qself_range(callee),
            expr: callee,
        };
        let known = match self.body.exprs[callee] {
            // `S::f`, `<S>::f`: found on a type written in the path.
            Expr::Path(Res::Assoc { ty: Some(_), .. }) => target,
            _ => target.filter(|_| {
                self.body
                    .generic_args
                    .get(callee)
                    .is_some_and(|g| g.ty.is_some())
            }),
        };
        self.register_call_bounds(function, &generic_args, site, known);
        if data.is_unsafe && self.unsafe_depth == 0 {
            self.error(
                self.range(id),
                format!("call to unsafe function `{name}` requires an `unsafe` block"),
            );
        }
        self.check_signature_privacy(sig, id);
        if let Some(context) = self.const_context
            && !data.is_const
        {
            let what = if data.owner.is_some() {
                "associated function"
            } else {
                "function"
            };
            let name = self.items.function_path(data);
            self.error(
                self.range(id),
                format!("cannot call non-const {what} `{name}` in {context}"),
            );
        }
        sig.ret
    }

    /// The function a path names, `function` as resolved, and the type
    /// its block must be for, if the path says: of a generic struct's
    /// blocks with an associated function of one name, the one for that
    /// type. `None` if there is none (reported).
    fn select_path_fn(
        &mut self,
        callee: ExprId,
        function: FunctionId,
    ) -> Option<(FunctionId, Option<Ty>, Option<Vec<Ty>>)> {
        let items = self.items;
        let data = &items.functions[function];
        let target = self.path_target(callee, function);
        let segment = self.source_map.segments.get(callee).copied();
        let segment = segment.unwrap_or_else(|| self.range(callee));
        let selected = match items.impl_of(function).and_then(|o| items.impls[o].target) {
            Some(target_struct @ flux_hir::ImplTarget::Struct(s))
                if !items.structs[s].generics.is_empty() =>
            {
                let candidates = items.assoc.get(&(target_struct, data.name));
                let candidates = candidates.map_or(&[][..], |v| v);
                let candidates = candidates.iter().filter_map(|&item| match item {
                    flux_hir::AssocItem::Function(f) => Some(f),
                    flux_hir::AssocItem::Const(_) => None,
                });
                self.select(candidates, target)
            }
            _ => Ok(function),
        };
        match selected {
            Ok(function) => Some((function, target, None)),
            Err(0) => {
                let what = target.map_or("this type".to_owned(), |t| {
                    format!("struct `{}`", self.describe(t))
                });
                self.error(
                    segment,
                    format!(
                        "no associated function or constant named `{}` found for {what} in the current scope",
                        data.name
                    ),
                );
                None
            }
            Err(_) => {
                self.error(segment, "multiple applicable items in scope");
                None
            }
        }
    }

    /// `<S as Tr>`'s `S` in a qualified path `callee`.
    fn qself_range(&self, callee: ExprId) -> Option<TextRange> {
        match self.body.exprs[callee] {
            Expr::Path(Res::Assoc {
                ty: Some(ty),
                trait_: Some(_),
                ..
            }) => self.source_map.types.get(ty).copied(),
            _ => None,
        }
    }

    /// A function may be called only where every struct in its signature
    /// is visible: `pub fn f() -> P` with a private `P` is callable only
    /// where `P` is.
    fn check_signature_privacy(&mut self, sig: &FnSig, call: ExprId) {
        fn structs(ty: Ty, out: &mut Vec<Ty>) {
            match ty.kind() {
                TyKind::Adt(_, args) => {
                    out.push(ty);
                    args.iter().for_each(|&a| structs(a, out));
                }
                TyKind::Tuple(elements) => elements.iter().for_each(|&e| structs(e, out)),
                TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => structs(t, out),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for &ty in sig.params.iter().chain([&sig.ret]) {
            structs(ty, &mut found);
        }
        let hidden = found.into_iter().find(|&ty| match ty.kind() {
            TyKind::Adt(adt, _) => !self
                .items
                .is_visible(self.items.structs[adt.strukt].loc.vis, self.module),
            _ => false,
        });
        if let Some(ty) = hidden {
            let name = self.describe(ty);
            self.error(self.range(call), format!("type `{name}` is private"));
        }
    }

    /// `P(a, b)`, a tuple struct's constructor.
    fn infer_constructor(
        &mut self,
        call: ExprId,
        callee: ExprId,
        strukt: StructId,
        args: &[ExprId],
    ) -> Ty {
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
        let ty = self.instantiate_struct(strukt, Some(callee), self.range(callee));
        self.meet_expectation(call, ty);
        let fields = sigs.fields(ty).unwrap_or(&[]);
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
            if let Some(&field) = fields.get(i) {
                self.expect(arg, field);
            }
            let ty = self.infer_expr(arg);
            if let Some(&field) = fields.get(i) {
                self.coerce(ty, field, arg);
            }
        }
        ty
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
                let operand = match self.body.exprs[id] {
                    Expr::Unary { operand, .. } => operand,
                    _ => id,
                };
                self.annotations_needed(operand, var, self.range(id));
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
            TyKind::Adt(adt, args) => items.structs[adt.strukt]
                .field(name)
                .map(|(i, _)| (i, sigs.field_types(adt.strukt, args)[i])),
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
            TyKind::Infer(var) => {
                self.annotations_needed(base, var, self.range(base));
                return Ty::ERROR;
            }
            _ => None,
        };
        match found {
            Some((index, ty)) => {
                self.field_indices.insert(id, index as u32);
                if let TyKind::Adt(adt, _) = base_ty.kind() {
                    let def = &items.structs[adt.strukt];
                    if !items.is_visible(def.fields[index].vis, self.module) {
                        self.error(
                            range,
                            format!("field `{name}` of struct `{}` is private", def.name),
                        );
                    }
                }
                ty
            }
            None => {
                // rustc names the type before dereferencing: `&S`.
                let is_method = match base_ty.kind() {
                    TyKind::Adt(adt, _) => items
                        .assoc_function(flux_hir::ImplTarget::Struct(adt.strukt), name)
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
        let range = self.range(id);
        let origin = self.source_map.paths.get(id).copied().unwrap_or(range);
        let ty = self.instantiate_struct(strukt, Some(id), origin);
        self.meet_expectation(id, ty);
        let field_types = sigs.fields(ty).unwrap_or(&[]);
        let names = self.source_map.expr_fields.get(id);
        let mut seen = Vec::new();
        let mut unknown = false;
        let mut private = Vec::new();
        for (i, &(name, value)) in fields.iter().enumerate() {
            if let Some((index, _)) = def.field(name) {
                self.expect(value, field_types[index]);
            }
            let value_ty = self.infer_expr(value);
            let name_range = names.and_then(|n| n.get(i)).copied().unwrap_or(range);
            if let Some((_, field)) = def.field(name)
                && !items.is_visible(field.vis, self.module)
                && !seen.contains(&name)
            {
                private.push((name, name_range));
            }
            if seen.contains(&name) {
                self.error(
                    name_range,
                    format!("field `{name}` specified more than once"),
                );
                continue;
            }
            seen.push(name);
            match def.field(name) {
                Some((index, _)) => self.coerce(value_ty, field_types[index], value),
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
                // The fields taken from `base` must be visible too.
                let base_range = self.range(base);
                for field in def.fields.iter() {
                    if !seen.contains(&field.name) && !items.is_visible(field.vis, self.module) {
                        private.push((field.name, base_range));
                    }
                }
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
        self.private_fields(def.name, &private);
        ty
    }

    /// Reports the fields of struct `name` that are private here, in one
    /// error, as rustc does.
    fn private_fields(&mut self, name: Symbol, private: &[(Symbol, TextRange)]) {
        let Some(&(_, range)) = private.first() else {
            return;
        };
        let names: Vec<Symbol> = private.iter().map(|&(n, _)| n).collect();
        let message = match names.as_slice() {
            [one] => format!("field `{one}` of struct `{name}` is private"),
            _ => format!("{} of struct `{name}` are private", list_fields(&names)),
        };
        self.error(range, message);
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
        // Each type the receiver dereferences to, outermost first, as
        // rustc probes: methods whose `self` is that type, then a
        // reference to it; at each, its inherent methods before those of
        // traits.
        let mut base = receiver_ty;
        let mut derefs = 0;
        // The first type the method exists for, if only through blocks
        // whose bounds do not hold.
        let mut unmet = None;
        let items = self.items;
        let traits = items.trait_fns.contains_key(&method);
        let inherent = |ty: Ty| {
            impl_target(ty)
                .and_then(|t| items.assoc.get(&(t, method)))
                .map_or(&[][..], |v| v)
        };
        loop {
            // `&self` on a `&T` receiver passes it on.
            let through = match base.kind() {
                TyKind::Ref(m, pointee) => Some((self.shallow(pointee), derefs + 1, Some(m))),
                _ => None,
            };
            let tries = [
                Some((base, derefs, None)),
                through,
                Some((base, derefs, Some(Mutability::Not))),
                Some((base, derefs, Some(Mutability::Mut))),
            ];
            for (ty, d, mode) in tries.into_iter().flatten() {
                match self.probe_method(ty, mode, method, inherent(ty), traits) {
                    Ok(Some((function, trait_args))) => {
                        return self.check_method(
                            id,
                            receiver,
                            function,
                            (ty, d),
                            trait_args,
                            args,
                            name_range,
                        );
                    }
                    Ok(None) => {}
                    Err(()) => {
                        self.error(name_range, "multiple applicable items in scope");
                        return Ty::ERROR;
                    }
                }
            }
            if unmet.is_none() && self.exists_unmet(base, method, true) {
                unmet = Some(base);
            }
            match self.shallow(base).kind() {
                TyKind::Ref(_, pointee) => {
                    base = self.shallow(pointee);
                    derefs += 1;
                }
                _ => break,
            }
        }
        if let Some(ty) = unmet {
            let what = self.describe_kind(ty);
            self.error(
                name_range,
                format!(
                    "the method `{method}` exists for {what}, but its trait bounds were not satisfied"
                ),
            );
            return Ty::ERROR;
        }
        // A method some trait has is missing, as rustc would say;
        // flux's `core` has only some of the primitive types' methods.
        let in_some_trait = self.items.traits.values().any(|t| {
            t.function(self.items, method)
                .is_some_and(|f| self.items.functions[f].has_self)
        });
        let other = matches!(base.kind(), TyKind::Tuple(_) | TyKind::RawPtr(..));
        if in_some_trait && (other || impl_target(base).is_some()) || base.has_params() {
            let what = match base.kind() {
                TyKind::Adt(adt, _)
                    if receiver_ty == base && self.other_instance_has(base, method) =>
                {
                    let generics = &self.items.structs[adt.strukt].generics;
                    format!("struct `{}{generics}`", adt.name)
                }
                _ => self.describe_kind(receiver_ty),
            };
            self.error(
                name_range,
                format!("no method named `{method}` found for {what} in the current scope"),
            );
            return Ty::ERROR;
        }
        if let Some(target) = impl_target(base) {
            if matches!(target, flux_hir::ImplTarget::Primitive(_)) {
                let what = self.describe(receiver_ty);
                self.error(
                    name_range,
                    format!("the method `{method}` on `{what}` is not supported yet"),
                );
                return Ty::ERROR;
            }
            let what = self.describe_kind(receiver_ty);
            self.error(
                name_range,
                format!("no method named `{method}` found for {what} in the current scope"),
            );
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
        let message = match receiver_ty.kind() {
            TyKind::Int(_) | TyKind::Bool | TyKind::Tuple(_) | TyKind::RawPtr(..) => {
                format!("methods on `{ty}` are not supported yet")
            }
            _ => format!("the method `{method}` on `{ty}` is not supported yet"),
        };
        self.error(name_range, message);
        Ty::ERROR
    }

    /// The method `name` whose `self` is `ty`, taken as `mode` says:
    /// inherent, else of a trait (with the trait's generic arguments if
    /// a bound gives them). `Err` if several are (E0034).
    #[allow(clippy::type_complexity)]
    fn probe_method(
        &mut self,
        ty: Ty,
        mode: Option<Mutability>,
        name: Symbol,
        inherent: &[flux_hir::AssocItem],
        traits: bool,
    ) -> Result<Option<(FunctionId, Option<crate::GenericArgs>)>, ()> {
        let items = self.items;
        if !inherent.is_empty() {
            let candidates = inherent.iter().filter_map(|&item| match item {
                flux_hir::AssocItem::Function(f)
                    if items.functions[f].has_self && self_mode(items, f) == mode =>
                {
                    Some(f)
                }
                _ => None,
            });
            match self.select(candidates, Some(ty)) {
                Ok(function) => return Ok(Some((function, None))),
                Err(2..) => return Err(()),
                Err(_) => {}
            }
        }
        if !traits {
            return Ok(None);
        }
        let found: Vec<_> = self
            .trait_items(ty, name, true)
            .into_iter()
            .filter(|&(f, _)| self_mode(items, f) == mode)
            .collect();
        match found.as_slice() {
            [] => Ok(None),
            [(function, trait_ref)] => Ok(Some((*function, trait_ref.map(|t| t.args)))),
            _ => Err(()),
        }
    }

    /// A call of `function`, a method, on a receiver of type `base` once
    /// `derefs` references are taken off it.
    #[allow(clippy::too_many_arguments)]
    fn check_method(
        &mut self,
        id: ExprId,
        receiver: ExprId,
        function: FunctionId,
        (base, derefs): (Ty, u32),
        trait_args: Option<&[Ty]>,
        args: &[ExprId],
        name_range: TextRange,
    ) -> Ty {
        let data = &self.items.functions[function];
        let generic_args = self.instantiate_fn(id, function, Some(base), trait_args, name_range);
        if !self.items.is_visible(data.loc.vis, self.module) {
            self.error(name_range, format!("method `{}` is private", data.name));
        }
        let sig = &self.instance_sig(function, &generic_args);
        self.check_signature_privacy(sig, id);
        if let Some(context) = self.const_context
            && !data.is_const
        {
            let name = self.items.function_path(data);
            let range = TextRange::new(name_range.start(), self.range(id).end());
            self.error(
                range,
                format!("cannot call non-const method `{name}` in {context}"),
            );
        }
        // The bounds first: an `impl` block they select may tell the
        // trait's generic arguments, and so the return type.
        let site = CallSite {
            callee: name_range,
            whole: self.range(id),
            receiver: Some(receiver),
            args,
            qself: None,
            expr: id,
        };
        self.register_call_bounds(function, &generic_args, site, Some(base));
        let sig = &self.instance_sig(function, &generic_args);
        self.meet_expectation(id, sig.ret);
        let autoref = self_mode(self.items, function);
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
        self.select_obligations();
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
                if let Some(ty) = self.overload(id, op, ty, (rhs, rhs_ty)) {
                    return ty;
                }
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
                if arith.is_none() {
                    self.check_place(id, lhs, true);
                }
                let ty = self.infer_expr(lhs);
                match arith {
                    None => {
                        self.expect(rhs, ty);
                        let rhs_ty = self.infer_expr(rhs);
                        self.coerce(rhs_ty, ty, rhs);
                    }
                    Some(arith) => {
                        self.infer_arith(id, op, arith, lhs, rhs);
                        // An overloaded one borrows `lhs` mutably, which
                        // the borrow check checks.
                        if self.overloads.get(id).is_none() {
                            self.check_place(id, lhs, false);
                        }
                    }
                }
                Ty::UNIT
            }
        }
    }

    /// `lhs op rhs` where `lhs` is a struct, a generic parameter or an
    /// associated type: a call of the method of the operator's trait
    /// (`Add::add`, `PartialEq::eq`, `AddAssign::add_assign`), whose
    /// bound `L: Add<R>` is an obligation, with the type of the result;
    /// `None` for the built-in operators. As in rustc, the right operand
    /// is checked against what the `impl` block selected says it is.
    fn overload(
        &mut self,
        id: ExprId,
        op: BinaryOp,
        lhs: Ty,
        (rhs, rhs_ty): (ExprId, Ty),
    ) -> Option<Ty> {
        let operand = self.shallow(lhs);
        let overloadable = match operand.kind() {
            TyKind::Adt(..) | TyKind::Param(_) | TyKind::Alias(_) => true,
            // `&a + &b` on integers needs `core`'s impls for references.
            TyKind::Ref(_, pointee) => matches!(
                self.shallow(pointee).kind(),
                TyKind::Adt(..) | TyKind::Param(_) | TyKind::Alias(_)
            ),
            _ => false,
        };
        let op_range = self.source_map.ops.get(id).copied().unwrap_or_default();
        if let BinaryOp::Cmp(_) = op
            && let Some(message) = self.cmp_ref_error(op, operand, rhs_ty)
        {
            self.error(op_range, message);
            return Some(Ty::BOOL);
        }
        if !overloadable {
            // An integer or `bool` on the left and another type on the
            // right: no `impl` block of `core`'s takes it.
            let other = matches!(
                self.shallow(rhs_ty).kind(),
                TyKind::Adt(..) | TyKind::Param(_) | TyKind::Alias(_)
            );
            let primitive = self.is_primitive_operand(operand) && operand != Ty::ERROR;
            // Unless the program implements it: `impl Add<S> for i32`.
            if other
                && primitive
                && let Some((t, function)) = self.lang_fn(op)
                && self.probe(|cx| cx.may_hold(lhs, t, Some(&[rhs_ty]), 0))
            {
                let range = match op {
                    BinaryOp::Assign(_) => self.range(id),
                    _ => op_range,
                };
                let trait_ref = crate::TraitRef {
                    trait_: t,
                    name: self.items.traits[t].name,
                    args: Ty::list(&[rhs_ty]),
                };
                self.register(
                    trait_ref.with_self(lhs),
                    range,
                    solve::ObligationKind::Bound,
                );
                self.overloads.insert(id, (function, vec![lhs, rhs_ty]));
                self.select_obligations();
                return Some(match op {
                    BinaryOp::Cmp(_) => Ty::BOOL,
                    BinaryOp::Assign(_) => Ty::UNIT,
                    _ => self.output(trait_ref, lhs, range),
                });
            }
            if other && primitive && !matches!(op, BinaryOp::Assign(_)) {
                let (l, r) = (self.describe(lhs), self.describe(rhs_ty));
                let message = match op {
                    BinaryOp::Cmp(_) => format!("can't compare `{l}` with `{r}`"),
                    _ => binary_error(op, &l, &r),
                };
                self.error(op_range, message);
                return Some(match op {
                    BinaryOp::Cmp(_) => Ty::BOOL,
                    _ => Ty::ERROR,
                });
            }
            // Two integer types: `core` has no `impl Add<u8> for i32`.
            if let (Some(a), Some(b), BinaryOp::Arith(arith)) =
                (operand.as_int(), self.shallow(rhs_ty).as_int(), op)
                && a != b
                && !matches!(arith, ArithOp::Shl | ArithOp::Shr)
            {
                let (l, r) = (self.describe(lhs), self.describe(rhs_ty));
                self.error(op_range, binary_error(op, &l, &r));
            }
            return None;
        }
        let range = match op {
            BinaryOp::Assign(_) => self.range(id),
            _ => self.source_map.ops.get(id).copied().unwrap_or_default(),
        };
        let result = match op {
            BinaryOp::Cmp(_) => Ty::BOOL,
            BinaryOp::Assign(_) => Ty::UNIT,
            _ => Ty::ERROR,
        };
        // `&a == &b` compares `a` with `b`.
        let (mut lhs_cmp, mut rhs_cmp) = (lhs, rhs_ty);
        if let (BinaryOp::Cmp(_), TyKind::Ref(_, a), TyKind::Ref(_, b)) =
            (op, operand.kind(), self.shallow(rhs_ty).kind())
        {
            (lhs_cmp, rhs_cmp) = (a, b);
            self.compared_refs.insert(id, ());
        }
        let Some((t, function)) = self.lang_fn(op) else {
            let (lhs, rhs) = (self.describe(lhs), self.describe(rhs_ty));
            self.error(range, binary_error(op, &lhs, &rhs));
            return Some(result);
        };
        // Reported if not: its result is then an error.
        let holds = self.may_implement(lhs_cmp, t);
        // `&a == &b` where `a` has some `PartialEq`: through `core`'s
        // `impl PartialEq<&B> for &A where A: PartialEq<B>`, whose bound
        // is what fails.
        if holds && lhs_cmp != lhs {
            let trait_ref = crate::TraitRef {
                trait_: t,
                name: self.items.traits[t].name,
                args: Ty::list(&[rhs_cmp]),
            };
            self.register(
                trait_ref.with_self(lhs_cmp),
                range,
                solve::ObligationKind::Bound,
            );
            self.overloads
                .insert(id, (function, vec![lhs_cmp, rhs_cmp]));
            self.select_obligations();
            return Some(result);
        }
        let r = self.new_var(false, self.range(rhs));
        let trait_ref = crate::TraitRef {
            trait_: t,
            name: self.items.traits[t].name,
            args: Ty::list(&[r]),
        };
        self.register(
            trait_ref.with_self(lhs_cmp),
            range,
            solve::ObligationKind::Operator {
                op: solve::Op::Binary(op),
                lhs,
                rhs: rhs_ty,
            },
        );
        self.select_obligations();
        if matches!(self.shallow(r).kind(), TyKind::Infer(_)) {
            self.unify(r, rhs_cmp);
        } else if rhs_cmp == rhs_ty {
            self.coerce(rhs_ty, r, rhs);
        } else if !self.unify(r, rhs_cmp) {
            let range = self.range(rhs);
            self.mismatch(Ty::new(TyKind::Ref(Mutability::Not, r)), rhs_ty, range);
        }
        self.overloads.insert(id, (function, vec![lhs_cmp, r]));
        self.select_obligations();
        Some(match op {
            BinaryOp::Arith(_) if holds => self.output(trait_ref, lhs, range),
            _ => result,
        })
    }

    /// The trait of operator `op` and its method: `Add` and `Add::add`.
    fn lang_fn(&self, op: BinaryOp) -> Option<(flux_hir::TraitId, FunctionId)> {
        let (lang, method) = lang_method(op)?;
        let t = *self.items.lang_traits.get(&Symbol::intern(lang))?;
        let f = self.items.traits[t].function(self.items, Symbol::intern(method))?;
        Some((t, f))
    }

    /// rustc's error for comparing `lhs` with `rhs` where `core`'s impls
    /// for references cannot apply: a reference with something else
    /// ("can't compare `&S` with `S`", or E0369 if the referent has no
    /// `PartialEq` at all), or an integer of unknown type with a
    /// reference, which rustc names by its fallback type.
    fn cmp_ref_error(&mut self, op: BinaryOp, lhs: Ty, rhs: Ty) -> Option<String> {
        let rhs = self.shallow(rhs);
        let unknown = |cx: &mut Self, ty: Ty| match ty.kind() {
            TyKind::Infer(var) => Some(cx.unknown(var).0),
            _ => None,
        };
        let is_ref = |ty: Ty| matches!(ty.kind(), TyKind::Ref(..));
        let eq = *self.items.lang_traits.get(&Symbol::intern("eq"))?;
        match lhs.kind() {
            TyKind::Ref(_, pointee) if !is_ref(rhs) && unknown(self, rhs) != Some(false) => {
                if rhs == Ty::ERROR || pointee == Ty::ERROR {
                    return None;
                }
                let (l, r) = (self.describe(lhs), self.describe(rhs));
                Some(if self.may_implement(pointee, eq) {
                    format!("can't compare `{l}` with `{r}`")
                } else {
                    binary_error(op, &l, &r)
                })
            }
            TyKind::Infer(_) if unknown(self, lhs) == Some(true) && is_ref(rhs) => {
                Some(format!("can't compare `i32` with `{}`", self.describe(rhs)))
            }
            _ => None,
        }
    }

    /// `-a` or `!a` where `a` is a struct, a generic parameter or an
    /// associated type: `Neg::neg(a)` or `Not::not(a)`.
    fn overload_unary(&mut self, id: ExprId, op: UnaryOp, ty: Ty) -> Option<Ty> {
        if !matches!(
            self.shallow(ty).kind(),
            TyKind::Adt(..) | TyKind::Param(_) | TyKind::Alias(_)
        ) {
            return None;
        }
        let range = self.range(id);
        let (lang, method, text) = match op {
            UnaryOp::Neg => ("neg", "neg", "-"),
            UnaryOp::Not => ("not", "not", "!"),
            UnaryOp::Deref => return None,
        };
        let found = self
            .items
            .lang_traits
            .get(&Symbol::intern(lang))
            .and_then(|&t| {
                Some((
                    t,
                    self.items.traits[t].function(self.items, Symbol::intern(method))?,
                ))
            });
        let Some((t, function)) = found else {
            let ty = self.describe(ty);
            self.error(
                range,
                format!("cannot apply unary operator `{text}` to type `{ty}`"),
            );
            return Some(Ty::ERROR);
        };
        let holds = self.may_implement(ty, t);
        let trait_ref = crate::TraitRef {
            trait_: t,
            name: self.items.traits[t].name,
            args: &[],
        };
        self.register(
            trait_ref.with_self(ty),
            range,
            solve::ObligationKind::Operator {
                op: solve::Op::Unary(op),
                lhs: ty,
                rhs: ty,
            },
        );
        self.overloads.insert(id, (function, vec![ty]));
        self.select_obligations();
        Some(if holds {
            self.output(trait_ref, ty, range)
        } else {
            Ty::ERROR
        })
    }

    /// `<ty as Tr>::Output`, normalized.
    fn output(&mut self, trait_ref: crate::TraitRef, ty: Ty, range: TextRange) -> Ty {
        let alias = crate::traits::alias_of(trait_ref, ty, Symbol::intern("Output"));
        self.normalize(Ty::alias(alias), range)
    }

    /// Whether the operators on a left operand of type `ty` are the
    /// built-in ones, whose right operand has the same type. Other types'
    /// operators come from traits, which say what the right operand is.
    fn is_primitive_operand(&mut self, ty: Ty) -> bool {
        !matches!(
            self.shallow(ty).kind(),
            TyKind::Ref(..) | TyKind::Adt(..) | TyKind::Tuple([_, ..]) | TyKind::RawPtr(..)
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
        if let Some(ty) = self.overload(id, op, ty, (rhs, rhs_ty)) {
            return ty;
        }
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
        let mut reported = std::mem::take(&mut self.reported_vars);
        self.finish_obligations(&mut reported);
        self.report_unknown_lets(&mut reported);
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
        let instances: Vec<(ExprId, (FunctionId, Vec<Ty>))> = self
            .calls
            .iter()
            .map(|(e, call)| (e, call.clone()))
            .collect();
        let mut calls = ArenaMap::new();
        for (expr, (function, args)) in instances {
            let args: Vec<Ty> = args
                .iter()
                .map(|&a| self.resolve(a, &mut reported))
                .collect();
            let args = Ty::list(&args);
            // Without lifetimes, a reference a call returns cannot be
            // tied to what it borrows.
            if crate::contains_ref(self.sigs[function].ret.subst(args)) {
                self.error(
                    self.range(expr),
                    "calls returning references are not supported yet",
                );
            }
            calls.insert(expr, Instance { function, args });
        }
        let overloaded: Vec<(ExprId, (FunctionId, Vec<Ty>))> = self
            .overloads
            .iter()
            .map(|(e, call)| (e, call.clone()))
            .collect();
        let mut overloads = ArenaMap::new();
        for (expr, (function, args)) in overloaded {
            let args: Vec<Ty> = args
                .iter()
                .map(|&a| self.resolve(a, &mut reported))
                .collect();
            let args = Ty::list(&args);
            overloads.insert(expr, Instance { function, args });
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
            calls,
            overloads,
            compared_refs: self.compared_refs,
            diagnostics: self.diagnostics,
        }
    }

    /// Reports that `expr`'s type, variable `var`, must be known by now:
    /// at the `let` that bound it if it is a local declared without a
    /// type, as rustc does, else at `range`.
    fn annotations_needed(&mut self, expr: ExprId, var: InferVar, range: TextRange) {
        let root = self.root(var);
        if !self.reported_vars.insert(root) {
            return;
        }
        let body = self.body;
        let let_pat = match body.exprs[self.blame(expr)] {
            Expr::Path(Res::Local(binding)) => body.exprs.values().find_map(|e| {
                let Expr::Block { stmts, .. } = e else {
                    return None;
                };
                stmts.iter().find_map(|stmt| match *stmt {
                    Stmt::Let { pat, ty: None, .. } => {
                        let mut bound = Vec::new();
                        body.pat_bindings(pat, &mut bound);
                        bound.contains(&binding).then_some(pat)
                    }
                    _ => None,
                })
            }),
            _ => None,
        };
        let range = let_pat
            .and_then(|pat| self.source_map.pats.get(pat).copied())
            .unwrap_or(range);
        self.error(range, "type annotations needed");
    }

    /// Reports `let` bindings without a type whose type is not fully
    /// known, as rustc does, rather than where the unknown part came
    /// from: `let x = f();` for a generic `f`.
    fn report_unknown_lets(&mut self, reported: &mut HashSet<InferVar>) {
        let body = self.body;
        for (_, expr) in body.exprs.iter() {
            let Expr::Block { stmts, .. } = expr else {
                continue;
            };
            for stmt in stmts.iter() {
                let Stmt::Let {
                    pat,
                    ty: None,
                    init: Some(init),
                } = *stmt
                else {
                    continue;
                };
                let ty = self.expr_types.get(init).copied().unwrap_or(Ty::ERROR);
                let mut unknown = Vec::new();
                self.unknown_vars(ty, &mut unknown);
                if unknown.is_empty() || unknown.iter().any(|v| reported.contains(v)) {
                    continue;
                }
                let message = match self.shallow(ty).kind() {
                    TyKind::Infer(_) => "type annotations needed".to_owned(),
                    _ => format!("type annotations needed for `{}`", self.describe(ty)),
                };
                let range = self.source_map.pats.get(pat).copied().unwrap_or_default();
                self.error(range, message);
                reported.extend(unknown);
            }
        }
    }

    /// The variables in `ty` that nothing gave a type and that have no
    /// default.
    fn unknown_vars(&mut self, ty: Ty, out: &mut Vec<InferVar>) {
        let ty = self.shallow(ty);
        match ty.kind() {
            TyKind::Infer(var) => {
                if self.unknown(var) == (false, false) {
                    out.push(var);
                }
            }
            TyKind::Tuple(list) | TyKind::Adt(_, list) => {
                for &t in list {
                    self.unknown_vars(t, out);
                }
            }
            TyKind::Alias(alias) => {
                for &t in alias.args {
                    self.unknown_vars(t, out);
                }
            }
            TyKind::RawPtr(_, t) | TyKind::Ref(_, t) => self.unknown_vars(t, out),
            _ => {}
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
                        if reported.insert(var) && !self.tainted {
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
            TyKind::Adt(adt, args) if ty.has_infer() => {
                let args: Vec<Ty> = args.iter().map(|&a| self.resolve(a, reported)).collect();
                Ty::adt(adt.strukt, adt.name, &args)
            }
            TyKind::Alias(alias) if ty.has_infer() => {
                let args: Vec<Ty> = alias
                    .args
                    .iter()
                    .map(|&a| self.resolve(a, reported))
                    .collect();
                Ty::alias(crate::AliasTy {
                    args: Ty::list(&args),
                    ..alias
                })
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
                            binary_error(op, &ty, &rhs)
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
                    let to = self.resolve(to, reported);
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
                    if matches!(ty.kind(), TyKind::Adt(..) | TyKind::Tuple([_, ..])) {
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

/// `function`'s signature with generic arguments `args`.
/// How method `f` takes `self`: by value (`None`) or by reference.
fn self_mode(items: &ItemTree, f: FunctionId) -> Option<Mutability> {
    match items.functions[f].params.first().map(|p| &p.ty) {
        Some(flux_hir::TypeRef::Ref { mutable, .. }) => Some(if *mutable {
            Mutability::Mut
        } else {
            Mutability::Not
        }),
        _ => None,
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
/// The `lang` name of the trait for `op` on types other than integers
/// and `bool`, and its method's name; `None` if flux has none.
fn lang_method(op: BinaryOp) -> Option<(&'static str, &'static str)> {
    Some(match op {
        BinaryOp::Arith(arith) | BinaryOp::Assign(Some(arith)) => {
            let names = match arith {
                ArithOp::Add => ("add", "add_assign"),
                ArithOp::Sub => ("sub", "sub_assign"),
                ArithOp::Mul => ("mul", "mul_assign"),
                ArithOp::Div => ("div", "div_assign"),
                ArithOp::Rem => ("rem", "rem_assign"),
                ArithOp::BitAnd => ("bitand", "bitand_assign"),
                ArithOp::BitOr => ("bitor", "bitor_assign"),
                ArithOp::BitXor => ("bitxor", "bitxor_assign"),
                ArithOp::Shl => ("shl", "shl_assign"),
                ArithOp::Shr => ("shr", "shr_assign"),
            };
            if matches!(op, BinaryOp::Assign(_)) {
                (names.1, names.1)
            } else {
                (names.0, names.0)
            }
        }
        BinaryOp::Cmp(CmpOp::Eq) => ("eq", "eq"),
        BinaryOp::Cmp(CmpOp::Ne) => ("eq", "ne"),
        _ => return None,
    })
}

fn binary_error(op: BinaryOp, lhs: &dyn std::fmt::Display, rhs: &dyn std::fmt::Display) -> String {
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
