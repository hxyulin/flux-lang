//! Type inference for one body.

mod closure;
mod solve;

use std::collections::{HashMap, HashSet};

use flux_arena::ArenaMap;
use flux_hir::{
    ArithOp, AssocItem, BinaryOp, Binding, BindingId, Body, BodySourceMap, CmpOp, ConstId,
    Diagnostic, Expr, ExprId, FunctionId, Generics, ItemTree, Literal, LogicOp, LoopSource, Pat,
    PatId, Res, SegmentArgs, Stmt, StructId, StructKind, TypeRefId, UnaryOp,
};
use flux_intern::Symbol;
use flux_syntax::TextRange;

use crate::traits::{Predicate, elaborate, predicates_of};
use crate::ty::{InferVar, IntTy, Mutability, Ty, TyKind};
use crate::{
    ConstInstance, FnSig, Instance, Signatures, TypeScope, function_name, generic_names,
    impl_target, resolve_or_report, self_ty, wrong_arg_count,
};
pub use closure::{Capture, CaptureMode, CaptureProj, ClosureInfo};

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
    /// How many references a `Field` or `Index` expression's base is
    /// dereferenced through first: one for `r.a` with `r: &S`, or `r[0]`
    /// with `r: &[i32]`.
    pub field_derefs: ArenaMap<Expr, u32>,
    /// Indexing expressions whose base, an array, is borrowed and unsized
    /// to a slice for `Index::index` (`a[1..3]`), past the dereferences
    /// in `field_derefs`.
    pub unsized_indices: ArenaMap<Expr, ()>,
    /// The method each `MethodCall` calls, and how its receiver is passed.
    pub methods: ArenaMap<Expr, Method>,
    /// Expressions of type `&mut T` that are reborrowed where they are
    /// used, rather than moved: `&*r` or `&mut *r` with this mutability.
    /// rustc does this where a reference is expected.
    pub reborrows: ArenaMap<Expr, Mutability>,
    /// Of `reborrows`, those that go through more than one reference,
    /// with how many: `&**r` (2) for a `&&T` where a `&T` is expected,
    /// rustc's deref coercion.
    pub deref_coercions: ArenaMap<Expr, u32>,
    /// Expressions of a function item's type that coerce to a function
    /// pointer where they are used: `let f: fn(i32) -> i32 = double;`.
    pub reified: ArenaMap<Expr, ()>,
    /// The function each call calls, with its generic arguments: at the
    /// callee `Path`, or the `MethodCall`. Not always the path's: of the
    /// associated functions of a generic struct with one name, the one
    /// for the type it is called on.
    pub calls: ArenaMap<Expr, Instance>,
    /// The trait method each operator on a type other than the integers
    /// and `bool` calls: `Add::add` for `a + b`, `PartialEq::eq` for
    /// `a == b`, `PartialOrd::lt` for `a < b`, `AddAssign::add_assign`
    /// for `a += b`, `Neg::neg` for `-a`.
    pub overloads: ArenaMap<Expr, Instance>,
    /// The associated constant each path to one of a trait or a generic
    /// `impl` block names (`T::N`, `<S as Tr>::N`, `Self::N`), with its
    /// owner's generic arguments. Paths to other constants name them
    /// alone.
    pub consts: ArenaMap<Expr, ConstInstance>,
    /// Comparisons of references, `&a == &b` or `&a < &b`, whose operands
    /// are passed as they are: `PartialEq::eq::<A, B>(a, b)`, as `core`'s
    /// impls for references would.
    pub compared_refs: ArenaMap<Expr, ()>,
    /// The conversion of the error each `?` on a `Result` returns,
    /// `From::from::<F, E>`, where the function's error type `F` is not
    /// `E` itself.
    pub try_conversions: ArenaMap<Expr, Instance>,
    /// The type of the value each pattern matches, after the references
    /// it sees through.
    pub pat_types: ArenaMap<Pat, Ty>,
    /// How many references a pattern that is not a binding sees through
    /// before it matches: one for `Some(x)` matching a `&Option<i32>`.
    pub pat_derefs: ArenaMap<Pat, u32>,
    /// The bindings that bind a reference to what they match rather than
    /// its value: `ref x`, or `x` in a pattern that sees through a
    /// reference.
    pub binding_refs: ArenaMap<Binding, Mutability>,
    /// The checks of the body's patterns wait for the values of the
    /// constants its patterns name: [`crate::check_deferred_patterns`].
    pub deferred_pats: bool,
    /// Expressions of type `&[T; N]` or `&mut [T; N]` used where a slice
    /// is expected, with the type they become (`&[T]`), after the
    /// reborrow in `reborrows` if there is one: rustc's unsizing
    /// coercion.
    pub unsizes: ArenaMap<Expr, Ty>,
    /// The built-in methods of arrays and slices called, with how many
    /// references the receiver is dereferenced through first.
    pub builtin_methods: ArenaMap<Expr, (BuiltinMethod, u32)>,
    /// The constants array lengths in the body name whose values were
    /// not known: the body is inferred again once they are.
    pub missing_lens: Vec<ConstId>,
    /// What each closure expression captures, and its kind. A call of a
    /// value (`f(x)`, a closure or a type parameter) is in `methods`
    /// and `calls` as a call of its `Fn`-family trait's method, with the
    /// arguments as a tuple.
    pub closures: ArenaMap<Expr, ClosureInfo>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A method of arrays and slices built into the compiler, until `core`
/// can define it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinMethod {
    /// `len`
    Len,
    /// `is_empty`
    IsEmpty,
}

/// How a binding binds what it matches, unless it says otherwise: by
/// value, or once a pattern has seen through a reference, by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingMode {
    Move,
    Ref(Mutability),
}

/// A resolved method call: the receiver is dereferenced `derefs` times,
/// then borrowed (`autoref`) or passed by value, and the reference to an
/// array unsized to one to a slice if `unsize` (`arr.iter()`), as
/// rustc's probe does past the last dereference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Method {
    pub function: FunctionId,
    pub derefs: u32,
    pub autoref: Option<Mutability>,
    pub unsize: bool,
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
        function: Some(function),
        module: data.loc.module,
        self_ty: self_ty(sigs, data.owner),
        generics: generic_names(items, data.owner, &data.generics),
        is_unsafe: data.is_unsafe,
        // `core`'s panic aborts, which constant evaluation does not run.
        const_context: (data.is_const && items.panic_fn != Some(function))
            .then_some("constant functions"),
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
    // A discriminant's `Self` is its enum, which is not generic.
    let enum_ty = data.discriminant.map(|(strukt, _)| {
        let name = items.structs[strukt].name;
        Ty::adt(strukt, name, &[])
    });
    let owner = Owner {
        function: None,
        module: data.loc.module,
        self_ty: enum_ty.or_else(|| self_ty(sigs, data.owner)),
        generics: generic_names(items, data.owner, &Generics::default()),
        is_unsafe: false,
        const_context: Some(if data.is_static.is_some() {
            "statics"
        } else {
            "constants"
        }),
        ret,
        param_env: elaborate(
            sigs,
            &crate::traits::owner_predicates(items, sigs, data.owner),
        ),
        env_projections: crate::traits::owner_projections(items, sigs, data.owner),
        self_trait: data
            .owner
            .and_then(flux_hir::Owner::as_impl)
            .and_then(|imp| sigs.impl_traits.get(imp).copied()),
    };
    let mut ctx = Ctx::new(items, sigs, owner, body, source_map);
    // What the constant's type needs is reported with it, not again in
    // its value, as rustc does; but the traits of its associated types
    // are needed again to normalize it, at the value.
    ctx.signature_wf(ret);
    if ret.has_aliases() {
        let range = ctx.range(body.root);
        ctx.require_alias_traits(ret, None, range);
    }
    let ret = ctx.normalize(ret, data.def_range);
    ctx.expect(body.root, ret);
    let ty = ctx.infer_expr(body.root);
    ctx.coerce(ty, ret, body.root);
    ctx.finish()
}

/// What a body belongs to, as far as inference cares.
struct Owner {
    /// The function whose body it is; `None` for a constant's.
    function: Option<FunctionId>,
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
            owner_fn: owner.function,
            const_context: owner.const_context,
            body,
            source_map,
            vars: Vec::new(),
            expr_types: ArenaMap::new(),
            binding_types: ArenaMap::new(),
            field_indices: ArenaMap::new(),
            field_derefs: ArenaMap::new(),
            unsized_indices: ArenaMap::new(),
            methods: ArenaMap::new(),
            reborrows: ArenaMap::new(),
            deref_coercions: ArenaMap::new(),
            reified: ArenaMap::new(),
            calls: ArenaMap::new(),
            overloads: ArenaMap::new(),
            consts: ArenaMap::new(),
            compared_refs: ArenaMap::new(),
            try_conversions: ArenaMap::new(),
            pat_types: ArenaMap::new(),
            pat_derefs: ArenaMap::new(),
            binding_refs: ArenaMap::new(),
            mode_errors: Vec::new(),
            negated_pats: HashSet::new(),
            const_pats: HashSet::new(),
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
            normalizing: Vec::new(),
            obligations: Vec::new(),
            reported_unmet: HashSet::new(),
            written_wf: HashSet::new(),
            signature_wf: HashSet::new(),
            deferred: Vec::new(),
            unsizes: ArenaMap::new(),
            builtin_methods: ArenaMap::new(),
            lens: crate::ArrayLens::new(&sigs.array_lens),
            closures: Vec::new(),
            closure_calls: Vec::new(),
            kind_requirements: Vec::new(),
            closure_hints: ArenaMap::new(),
            mismatched_closures: HashSet::new(),
            closure_infos: Default::default(),
            closure_locals: Vec::new(),
            unit_matches: HashSet::new(),
            for_heads: HashSet::new(),
            reified_closures: Vec::new(),
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
    /// A call of a `core` function that rustc's `core` makes
    /// conditionally const, or of a trait's method, in a constant
    /// context: reported once its generic arguments are known, which say
    /// whether the `impl` block is one of `core`'s, `const` in rustc's.
    ConstUnstable {
        function: FunctionId,
        args: &'static [Ty],
        call: TextRange,
        whole: TextRange,
        context: &'static str,
    },
    /// A path to a constant in a pattern, `N` (not `in_range`) or an end
    /// of `A..=B`.
    ConstPat {
        expr: ExprId,
        in_range: bool,
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
    /// A range pattern, whose type must be an integer type.
    RangePat {
        pat: PatId,
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
    /// The function whose body this is, whose closures' types name it.
    owner_fn: Option<FunctionId>,
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
    unsized_indices: ArenaMap<Expr, ()>,
    methods: ArenaMap<Expr, Method>,
    reborrows: ArenaMap<Expr, Mutability>,
    deref_coercions: ArenaMap<Expr, u32>,
    reified: ArenaMap<Expr, ()>,
    calls: ArenaMap<Expr, (FunctionId, Vec<Ty>)>,
    overloads: ArenaMap<Expr, (FunctionId, Vec<Ty>)>,
    consts: ArenaMap<Expr, (ConstId, Vec<Ty>)>,
    compared_refs: ArenaMap<Expr, ()>,
    try_conversions: ArenaMap<Expr, (FunctionId, Vec<Ty>)>,
    pat_types: ArenaMap<Pat, Ty>,
    pat_derefs: ArenaMap<Pat, u32>,
    binding_refs: ArenaMap<Binding, Mutability>,
    /// Binding modifiers and reference patterns written where a pattern
    /// borrows implicitly, which edition 2024 forbids: reported with the
    /// checks of patterns, after inference, as rustc does.
    mode_errors: Vec<(TextRange, &'static str)>,
    /// Negated literals in patterns, where `-` on an unsigned type is an
    /// unmet `Neg` bound, as rustc reports it.
    negated_pats: HashSet<ExprId>,
    /// Paths to constants in patterns, `N` and the ends of `A..=B`.
    const_pats: HashSet<ExprId>,
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
    /// The associated types being normalized, innermost last: one met
    /// again within its own normalization is rustc's overflow (E0275).
    normalizing: Vec<crate::ty::AliasTy>,
    /// Trait obligations not yet met.
    obligations: Vec<Obligation>,
    /// Unmet obligations reported, by range and message.
    reported_unmet: HashSet<(TextRange, String)>,
    /// The struct types written in the body, and in a constant, those its
    /// type names, whose bounds are checked once.
    written_wf: HashSet<Ty>,
    /// In a constant, the struct types its type names.
    signature_wf: HashSet<Ty>,
    deferred: Vec<Deferred>,
    unsizes: ArenaMap<Expr, Ty>,
    builtin_methods: ArenaMap<Expr, (BuiltinMethod, u32)>,
    /// The values of the constants array lengths name.
    lens: crate::ArrayLens<'a>,
    /// The closures inferred, whose captures are found once the body is.
    closures: Vec<closure::PendingClosure>,
    /// Calls of closures, whose methods follow from their kinds.
    closure_calls: Vec<closure::ClosureCall>,
    /// The `Fn`-family traits closures whose kinds are not known yet
    /// must implement, by their expressions.
    kind_requirements: Vec<(ExprId, crate::ClosureKind)>,
    /// The signatures closures passed to calls are expected to have.
    closure_hints: ArenaMap<Expr, closure::Hint>,
    /// Closures whose signature or kind was reported as wrong.
    mismatched_closures: HashSet<ExprId>,
    /// What each closure analyzed captures, innermost first.
    closure_infos: rustc_hash::FxHashMap<ExprId, ClosureInfo>,
    /// The bindings of the closures being inferred, innermost last.
    closure_locals: Vec<HashSet<BindingId>>,
    /// The `match`es of `for` loops, whose arms must be `()`.
    unit_matches: HashSet<ExprId>,
    /// The calls of `IntoIterator::into_iter` of `for` loops' heads.
    for_heads: HashSet<ExprId>,
    /// Closures coerced to function pointers, with their types and the
    /// pointers', which must capture nothing.
    reified_closures: Vec<(ExprId, Ty, Ty)>,
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

    /// `ty` with every known variable replaced, at any depth.
    fn known(&mut self, ty: Ty) -> Ty {
        let ty = self.shallow(ty);
        if !ty.has_infer() {
            return ty;
        }
        match ty.kind() {
            TyKind::RawPtr(m, pointee) => Ty::raw_ptr(m, self.known(pointee)),
            TyKind::Ref(m, pointee) => Ty::new(TyKind::Ref(m, self.known(pointee))),
            TyKind::Array(elem, len) => Ty::array(self.known(elem), len),
            TyKind::Slice(elem) => Ty::slice(self.known(elem)),
            TyKind::FnDef(_) | TyKind::FnPtr(_) => ty.map_fn(|t| self.known(t)),
            TyKind::Tuple(elements) => {
                let elements: Vec<Ty> = elements.iter().map(|&e| self.known(e)).collect();
                Ty::tuple(&elements)
            }
            TyKind::Adt(adt, args) => {
                let args: Vec<Ty> = args.iter().map(|&a| self.known(a)).collect();
                Ty::adt(adt.strukt, adt.name, &args)
            }
            TyKind::Closure(id, args) => {
                let args: Vec<Ty> = args.iter().map(|&a| self.known(a)).collect();
                Ty::closure(id, &args)
            }
            _ => ty,
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
            // The elements learn their type even if the lengths differ.
            (TyKind::Array(x, m), TyKind::Array(y, n)) => self.unify(x, y) && m == n,
            (TyKind::Slice(x), TyKind::Slice(y)) => self.unify(x, y),
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
            (TyKind::Closure(a, xs), TyKind::Closure(b, ys)) if a == b => {
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
            (TyKind::FnPtr(x), TyKind::FnPtr(y)) => {
                let mut ok =
                    x.is_unsafe == y.is_unsafe && x.inputs_output.len() == y.inputs_output.len();
                for (&a, &b) in x.inputs_output.iter().zip(y.inputs_output) {
                    ok &= self.unify(a, b);
                }
                ok
            }
            (TyKind::FnDef(x), TyKind::FnDef(y)) if x.function == y.function => {
                let mut ok = true;
                for (&a, &b) in x.args.iter().zip(y.args) {
                    ok &= self.unify(a, b);
                }
                for (&a, &b) in x.sig.inputs_output.iter().zip(y.sig.inputs_output) {
                    ok &= self.unify(a, b);
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
        // A reference to an array where one to a slice is expected is
        // unsized, after a reborrow if it is a `&mut` (which may become a
        // `&`): rustc's unsizing coercion.
        if let (TyKind::Ref(from, array), TyKind::Ref(to, slice)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
            && let (TyKind::Array(elem, _), TyKind::Slice(expected_elem)) =
                (self.shallow(array).kind(), self.shallow(slice).kind())
            && (from == to || from == Mutability::Mut)
        {
            if from == Mutability::Mut {
                self.reborrows.insert(expr, to);
            }
            self.unsizes.insert(expr, expected);
            if !self.unify(elem, expected_elem) {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A `&&T`, `&mut &T` or `&&mut T` where a `&T` is expected is
        // dereferenced until it is one, then reborrowed: rustc's deref
        // coercion, through references.
        if let (TyKind::Ref(from, pointee), TyKind::Ref(to, expected_pointee)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
            && (from == to || from == Mutability::Mut)
            && !self.can_unify(pointee, expected_pointee)
        {
            let (mut ty, mut derefs, mut unique) = (pointee, 1, from == Mutability::Mut);
            while let TyKind::Ref(m, inner) = self.shallow(ty).kind() {
                (ty, derefs) = (inner, derefs + 1);
                unique &= m == Mutability::Mut;
                if (to == Mutability::Not || unique) && self.can_unify(ty, expected_pointee) {
                    self.unify(ty, expected_pointee);
                    self.reborrows.insert(expr, to);
                    self.deref_coercions.insert(expr, derefs);
                    return;
                }
            }
        }
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
        // A closure where a function pointer is wanted becomes one, if
        // it captures nothing (checked once that is known).
        if let (TyKind::Closure(_, args), TyKind::FnPtr(ptr)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            let parts = crate::ClosureArgs::split(args);
            let params_ok = self.unify(parts.params(), Ty::tuple(ptr.params()));
            if params_ok && self.unify(parts.ret(), ptr.ret()) {
                self.reified.insert(expr, ());
                self.reified_closures.push((expr, actual, expected));
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A function item where a function pointer is wanted becomes
        // one, unless it is unsafe and the pointer is not.
        if let (TyKind::FnDef(def), TyKind::FnPtr(ptr)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            let extern_fn = self.items.functions[def.function].is_extern;
            let reified = Ty::new(TyKind::FnPtr(crate::FnSigTy {
                is_unsafe: ptr.is_unsafe,
                ..def.sig
            }));
            if !extern_fn && (ptr.is_unsafe || !def.sig.is_unsafe) && self.unify(reified, expected)
            {
                self.reified.insert(expr, ());
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A reference where a raw pointer is wanted becomes one, after
        // a reborrow if it is a `&mut`: `&T` to `*const T`, `&mut T` to
        // `*mut T` or `*const T`.
        if let (TyKind::Ref(from, pointee), TyKind::RawPtr(to, expected_pointee)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            if (from == to || from == Mutability::Mut) && self.unify(pointee, expected_pointee) {
                if from == Mutability::Mut {
                    self.reborrows.insert(expr, to);
                }
                self.unsizes.insert(expr, expected);
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
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
            if let Expr::Try { .. } = self.body.exprs[blamed] {
                let (expected, actual) = (self.describe(expected), self.describe(actual));
                self.error(
                    range,
                    format!(
                        "`?` operator has incompatible types: expected `{expected}`, found `{actual}`"
                    ),
                );
                return;
            }
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
            (TyKind::Array(x, m), TyKind::Array(y, n)) => m == n && self.can_unify(x, y),
            (TyKind::Slice(x), TyKind::Slice(y)) => self.can_unify(x, y),
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
                xs.len() == ys.len() && xs.iter().zip(ys).all(|(&x, &y)| self.can_unify(x, y))
            }
            (TyKind::Adt(a, xs), TyKind::Adt(b, ys)) => {
                a == b && xs.iter().zip(ys).all(|(&x, &y)| self.can_unify(x, y))
            }
            (TyKind::Closure(a, xs), TyKind::Closure(b, ys)) => {
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
            (TyKind::FnPtr(x), TyKind::FnPtr(y)) => {
                x.is_unsafe == y.is_unsafe
                    && x.inputs_output.len() == y.inputs_output.len()
                    && (x.inputs_output.iter().zip(y.inputs_output))
                        .all(|(&a, &b)| self.can_unify(a, b))
            }
            (TyKind::FnDef(x), TyKind::FnDef(y)) => {
                x.function == y.function
                    && x.args
                        .iter()
                        .zip(y.args)
                        .all(|(&a, &b)| self.can_unify(a, b))
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
            TyKind::Array(elem, len) => format!("[{}; {len}]", self.describe(elem)),
            TyKind::Slice(elem) => format!("[{}]", self.describe(elem)),
            TyKind::Tuple(elements) if !elements.is_empty() => {
                let elements: Vec<String> = elements.iter().map(|&e| self.describe(e)).collect();
                if elements.len() == 1 {
                    format!("({},)", elements[0])
                } else {
                    format!("({})", elements.join(", "))
                }
            }
            TyKind::Adt(adt, args) if !args.is_empty() => {
                let len = crate::ty::array_iter_len(adt.name, args);
                let mut args: Vec<String> = args.iter().map(|&a| self.describe(a)).collect();
                if let Some(len) = len {
                    args[1] = len.to_string();
                }
                format!("{}<{}>", adt.name, args.join(", "))
            }
            TyKind::FnPtr(sig) => self.describe_sig(sig),
            TyKind::FnDef(def) => {
                let mut text = format!("{} {{{}", self.describe_sig(def.sig), def.name);
                if !def.args.is_empty() {
                    let args: Vec<String> = def.args.iter().map(|&a| self.describe(a)).collect();
                    text.push_str(&format!("::<{}>", args.join(", ")));
                }
                text.push('}');
                text
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

    /// `fn(A, B) -> R` for messages, as [`Self::describe`] describes
    /// types.
    fn describe_sig(&mut self, sig: crate::FnSigTy) -> String {
        let params: Vec<String> = sig.params().iter().map(|&p| self.describe(p)).collect();
        let unsafety = if sig.is_unsafe { "unsafe " } else { "" };
        let mut text = format!("{unsafety}fn({})", params.join(", "));
        if self.shallow(sig.ret()) != Ty::UNIT {
            text.push_str(&format!(" -> {}", self.describe(sig.ret())));
        }
        text
    }

    fn mismatch(&mut self, expected: Ty, actual: Ty, range: TextRange) {
        if let (TyKind::Array(x, want), TyKind::Array(y, got)) =
            (self.shallow(expected).kind(), self.shallow(actual).kind())
            && want != got
            && self.can_unify(x, y)
        {
            self.error(
                range,
                format!(
                    "mismatched types: expected an array with a size of {want}, found one with a size of {got}"
                ),
            );
            return;
        }
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

    fn resolve_type_ref(&mut self, ty_ref: TypeRefId) -> Ty {
        let range = self
            .source_map
            .types
            .get(ty_ref)
            .copied()
            .unwrap_or_default();
        let scope = TypeScope {
            self_ty: self.self_ty,
            generics: &self.generics,
            signature_of: None,
            bounds: &self.param_env,
            self_trait: self.self_trait,
            self_types: &[],
            defaults: Some(&self.sigs.trait_defaults),
            lens: Some(&self.lens),
        };
        let ty = resolve_or_report(
            self.items,
            self.module,
            &self.body.types[ty_ref],
            scope,
            range,
            &mut self.diagnostics,
        );
        self.check_written_lifetimes(ty_ref, ty);
        let ty = self.fill_placeholders(ty, range);
        if ty.has_aliases() {
            let body = self.body;
            self.require_alias_traits(ty, Some(&body.types[ty_ref]), range);
        }
        let ty = self.normalize(ty, range);
        self.register_wf(ty, range, true);
        ty
    }

    /// Checks the lifetimes written in type `ty_ref` of the body, which
    /// resolved to `ty`: each must be in scope, and a struct's as many
    /// as it has. Elided ones are inferred by the borrow checker; flux
    /// does not enforce named ones in bodies yet.
    fn check_written_lifetimes(&mut self, ty_ref: TypeRefId, ty: Ty) {
        let written = &self.body.types[ty_ref];
        let mut found = Vec::new();
        crate::regions::lifetimes_in(written, &mut found);
        if found.is_empty() {
            return;
        }
        let mut names: Vec<Option<Symbol>> = Vec::new();
        let owner = match self.owner_fn {
            Some(f) => self.items.functions[f].owner,
            None => None,
        };
        if let Some(generics) = self.items.owner_generics(owner) {
            names.extend(generics.lifetimes.iter().map(|l| Some(l.name)));
        }
        if let Some(f) = self.owner_fn {
            let generics = &self.items.functions[f].generics;
            names.extend(generics.lifetimes.iter().map(|l| Some(l.name)));
        }
        let mut walker = crate::regions::Walker::new(self.items, self.module, names);
        let regions = walker.regions(written, ty);
        let errors = std::mem::take(&mut walker.errors);
        let unsupported =
            errors.is_empty() && regions.iter().any(|&r| r != crate::regions::Region::Erased);
        for (range, message) in errors {
            self.error(range, message);
        }
        if unsupported && let Some(l) = found.iter().find(|l| !l.is_anonymous()) {
            self.error(
                l.range,
                "named lifetimes in types in bodies are not supported yet",
            );
        }
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
            TyKind::FnPtr(_) => ty.map_fn(|t| self.fill_placeholders(t, origin)),
            TyKind::Ref(m, pointee) => {
                Ty::new(TyKind::Ref(m, self.fill_placeholders(pointee, origin)))
            }
            TyKind::Array(elem, len) => Ty::array(self.fill_placeholders(elem, origin), len),
            TyKind::Slice(elem) => Ty::slice(self.fill_placeholders(elem, origin)),
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

    /// Struct or enum `strukt` as used by expression `at`, whose generic
    /// arguments are those written there, `Self`'s, or new variables.
    fn instantiate_struct(
        &mut self,
        strukt: StructId,
        at: Option<ExprId>,
        origin: TextRange,
    ) -> Ty {
        let generics = at.and_then(|e| self.body.generic_args.get(e));
        self.instantiate_adt(strukt, generics, origin)
    }

    /// Struct or enum `strukt` with the generic arguments of a path,
    /// after the struct, or after the enum or its variant (`E::<T>::A`,
    /// `E::A::<T>`).
    fn instantiate_adt(
        &mut self,
        strukt: StructId,
        generics: Option<&flux_hir::PathGenerics>,
        origin: TextRange,
    ) -> Ty {
        let def = &self.items.structs[strukt];
        if generics.is_some_and(|g| g.self_type)
            && let Some(self_ty) = self.self_ty
        {
            return self_ty;
        }
        // Named through a type alias: its type.
        if let Some(alias) = generics.and_then(|g| g.alias) {
            let ty = self.resolve_type_ref(alias);
            return match ty.kind() {
                TyKind::Adt(adt, _) if adt.strukt == strukt => ty,
                _ => Ty::ERROR,
            };
        }
        let written = generics.and_then(|g| g.item.clone().or_else(|| g.ty.clone()));
        if def.generics.is_empty() && written.is_none() {
            return Ty::adt(strukt, def.name, &[]);
        }
        let kind = if def.is_enum() { "enum" } else { "struct" };
        let args = self.own_args(kind, def.generics.len(), written.as_ref(), origin);
        Ty::adt(strukt, def.name, &args)
    }

    /// The types of the fields of variant `variant` of `ty`, normalized.
    fn variant_fields_of(&mut self, ty: Ty, variant: u32, range: TextRange) -> &'static [Ty] {
        let fields = self.sigs.variant_fields(ty, variant).unwrap_or(&[]);
        if !fields.iter().any(|f| f.has_aliases()) {
            return fields;
        }
        let fields: Vec<Ty> = fields.iter().map(|&f| self.normalize(f, range)).collect();
        Ty::list(&fields)
    }

    /// The values of a variant's fields as the values of all the fields
    /// of its enum, for [`Self::register_struct_bounds`].
    fn variant_values(
        &self,
        strukt: StructId,
        variant: u32,
        values: Vec<Option<ExprId>>,
    ) -> Vec<Option<ExprId>> {
        let Some(starts) = self.sigs.variant_starts.get(strukt) else {
            return values;
        };
        let mut all = vec![None; *starts.last().unwrap_or(&0) as usize];
        let start = starts[variant as usize] as usize;
        for (i, v) in values.into_iter().enumerate() {
            all[start + i] = v;
        }
        all
    }

    /// The type whose block an associated function must be in, as the
    /// path `expr` naming it says: `S<A>` in `S::<A>::f`, `Self` in
    /// `Self::f`, or nothing.
    fn path_target(&mut self, expr: ExprId, function: FunctionId) -> Option<Ty> {
        let owner = self.items.impl_of(function)?;
        self.impl_path_target(expr, owner)
    }

    /// [`Self::path_target`] for an item of `impl` block `owner`.
    fn impl_path_target(&mut self, expr: ExprId, owner: flux_hir::ImplId) -> Option<Ty> {
        let generics = self.body.generic_args.get(expr)?;
        if generics.self_type {
            return self.self_ty;
        }
        let written = generics.ty.clone()?;
        let TyKind::Adt(adt, _) = self.sigs.impls.get(owner)?.kind() else {
            return None;
        };
        let want = self.items.structs[adt.strukt].generics.len();
        let args = self.own_args("struct", want, Some(&written), self.range(expr));
        let ty = Ty::adt(adt.strukt, adt.name, &args);
        // `W::<u8>`, which must meet `W`'s bounds.
        let range = TextRange::new(self.range(expr).start(), written.range.end());
        self.register_wf(ty, range, true);
        Some(ty)
    }

    /// "expected function, found `i32`", for a call of `callee`, of type
    /// `ty`.
    fn not_callable(&mut self, callee: ExprId, ty: Ty) {
        if ty == Ty::ERROR {
            return;
        }
        let ty = self.describe(ty);
        self.error(
            self.range(callee),
            format!("expected function, found `{ty}`"),
        );
    }

    /// The type of a path to constant `konst`, resolved by name: one of
    /// a generic `impl` block is for the type the path says, or one to
    /// infer.
    fn path_const(&mut self, expr: ExprId, konst: ConstId) -> Ty {
        let items = self.items;
        let Some(owner) = items.consts[konst].owner.and_then(flux_hir::Owner::as_impl) else {
            return self.sigs.consts.get(konst).copied().unwrap_or(Ty::ERROR);
        };
        if items.impls[owner].generics.is_empty() {
            return self.sigs.consts.get(konst).copied().unwrap_or(Ty::ERROR);
        }
        let target = self.impl_path_target(expr, owner);
        // Of blocks for different instances of a struct with a constant
        // of one name, the one for the type the path says.
        let name = items.consts[konst].name;
        let konst = match (items.impls[owner].target, name) {
            (Some(target_struct), Some(name)) => {
                let candidates = items.assoc.get(&(target_struct, name));
                let candidates: Vec<ConstId> = candidates
                    .map_or(&[][..], |v| v)
                    .iter()
                    .filter_map(|&item| match item {
                        AssocItem::Const(c) => Some(c),
                        AssocItem::Function(_) => None,
                    })
                    .collect();
                let mut found = candidates.iter().copied().filter(|&c| {
                    let imp = items.consts[c].owner.and_then(flux_hir::Owner::as_impl);
                    match (target, imp) {
                        (Some(target), Some(imp)) => {
                            let impl_ty = self.sigs.impls.get(imp).copied().unwrap_or(Ty::ERROR);
                            self.impl_applies(impl_ty, target)
                        }
                        _ => true,
                    }
                });
                match (found.next(), found.next()) {
                    (Some(c), None) => c,
                    (None, _) => {
                        let message = self.not_found(target, name);
                        let segment = self.source_map.segments.get(expr).copied();
                        self.error(segment.unwrap_or_else(|| self.range(expr)), message);
                        return Ty::ERROR;
                    }
                    (Some(_), Some(_)) => {
                        let segment = self.source_map.segments.get(expr).copied();
                        self.error(
                            segment.unwrap_or_else(|| self.range(expr)),
                            "multiple applicable items in scope",
                        );
                        return Ty::ERROR;
                    }
                }
            }
            _ => konst,
        };
        self.instantiate_const(expr, konst, target, None)
    }

    /// The type of associated constant `konst` as path `expr` names it,
    /// for type `target` if known (`Self`, for a trait's) and the trait's
    /// arguments if given, whose generic arguments it records.
    fn instantiate_const(
        &mut self,
        expr: ExprId,
        konst: ConstId,
        target: Option<Ty>,
        trait_args: Option<&[Ty]>,
    ) -> Ty {
        let (items, sigs) = (self.items, self.sigs);
        let data = &items.consts[konst];
        let range = self.range(expr);
        // A constant has no generic parameters of its own.
        let written = self
            .body
            .generic_args
            .get(expr)
            .and_then(|g| g.item.as_ref());
        if let Some(written) = written.filter(|w| !w.types.is_empty()) {
            let got = written.types.len();
            let (s, verb) = if got == 1 { ("", "was") } else { ("s", "were") };
            self.error(
                written.segment,
                format!(
                    "associated constant takes 0 generic arguments but {got} generic argument{s} {verb} supplied"
                ),
            );
        }
        let mut args = Vec::new();
        match data.owner {
            Some(flux_hir::Owner::Trait(t)) => {
                args.push(target.unwrap_or_else(|| self.new_var(false, range)));
                match trait_args {
                    Some(given) => args.extend_from_slice(given),
                    None => {
                        let count = items.traits[t].generics.len();
                        args.extend((0..count).map(|_| self.new_var(false, range)));
                    }
                }
            }
            Some(flux_hir::Owner::Impl(imp)) => {
                let count = items.impls[imp].generics.len();
                let vars: Vec<Ty> = (0..count).map(|_| self.new_var(false, range)).collect();
                let impl_ty = sigs
                    .impls
                    .get(imp)
                    .copied()
                    .unwrap_or(Ty::ERROR)
                    .subst(&vars);
                if let Some(target) = target {
                    self.unify(impl_ty, target);
                }
                args.extend(vars);
            }
            None => {}
        }
        self.register_const_bounds(expr, konst, &args, target);
        self.consts.insert(expr, (konst, args.clone()));
        let ty = sigs
            .consts
            .get(konst)
            .copied()
            .unwrap_or(Ty::ERROR)
            .subst(&args);
        self.normalize(ty, range)
    }

    /// The one of `candidates`, associated functions with one name, whose
    /// block may be for `target`: `Err` with how many are if not one.
    pub(super) fn select(
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
            (TyKind::Array(x, m), TyKind::Array(y, n)) => m == n && self.impl_applies(x, y),
            (TyKind::Slice(x), TyKind::Slice(y)) => self.impl_applies(x, y),
            (TyKind::FnPtr(x), TyKind::FnPtr(y)) => {
                x.is_unsafe == y.is_unsafe
                    && x.inputs_output.len() == y.inputs_output.len()
                    && (x.inputs_output.iter().zip(y.inputs_output))
                        .all(|(&a, &b)| self.impl_applies(a, b))
            }
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
        self.check_pat_in(pat, expected, BindingMode::Move, uninit);
    }

    /// Checks `pat` against a value of type `expected`, binding by
    /// `mode` unless a binding says otherwise. Patterns that are not
    /// bindings, wildcards or reference patterns see through references
    /// (`Some(x)` matching a `&Option<i32>`), and then bind by reference.
    fn check_pat_in(&mut self, pat: PatId, expected: Ty, mode: BindingMode, uninit: bool) {
        let (body, items) = (self.body, self.items);
        let range = self.source_map.pats.get(pat).copied().unwrap_or_default();
        // As in rustc, a literal of a reference type (`"a"`, `b"a"`)
        // matches a reference, so does not see through one.
        let reference_literal = |e: ExprId| {
            matches!(
                body.exprs[e],
                Expr::Literal(Literal::Str(_) | Literal::ByteStr(_) | Literal::CStr(_))
            )
        };
        let sees_through = match body.pats[pat] {
            Pat::Expr(e) => !reference_literal(e),
            Pat::Tuple { .. }
            | Pat::TupleStruct { .. }
            | Pat::Record { .. }
            | Pat::Unit(..)
            | Pat::Range { .. } => true,
            _ => false,
        };
        let (expected, mode) = if sees_through {
            self.peel_refs(pat, expected, mode)
        } else {
            (expected, mode)
        };
        self.pat_types.insert(pat, expected);
        match &body.pats[pat] {
            Pat::Missing | Pat::Wild => {}
            Pat::Bind { binding, sub } => {
                let data = &body.bindings[*binding];
                let own = body.alt_by_ref.get(pat).copied().unwrap_or(data.by_ref);
                if mode != BindingMode::Move && (own.is_some() || data.mutable) {
                    let message = if own.is_some() {
                        "cannot explicitly borrow within an implicitly-borrowing pattern"
                    } else {
                        "cannot mutably bind by value within an implicitly-borrowing pattern"
                    };
                    let at = TextRange::new(range.start(), (range.start() + 3).min(range.end()));
                    self.mode_errors.push((at, message));
                }
                let by_ref = match (own, mode) {
                    (Some(true), _) => Some(Mutability::Mut),
                    (Some(false), _) => Some(Mutability::Not),
                    (None, BindingMode::Move) => None,
                    (None, BindingMode::Ref(m)) => Some(m),
                };
                let ty = match by_ref {
                    Some(m) => Ty::new(TyKind::Ref(m, expected)),
                    None => expected,
                };
                // A later alternative of an or-pattern binds the same.
                match self.binding_types.get(*binding).copied() {
                    Some(bound) => {
                        if !self.unify(bound, ty) {
                            self.mismatch(bound, ty, range);
                        }
                    }
                    None => {
                        self.binding_types.insert(*binding, ty);
                    }
                }
                if let Some(m) = by_ref {
                    self.binding_refs.insert(*binding, m);
                }
                if uninit {
                    self.uninit_bindings.insert(*binding);
                }
                if let Some(sub) = sub {
                    self.check_pat_in(*sub, expected, mode, uninit);
                }
            }
            Pat::Ref {
                pat: inner,
                mutable,
            } => {
                if mode != BindingMode::Move {
                    let at = TextRange::new(range.start(), (range.start() + 1).min(range.end()));
                    self.mode_errors.push((
                        at,
                        "cannot explicitly dereference within an implicitly-borrowing pattern",
                    ));
                }
                let m = if *mutable {
                    Mutability::Mut
                } else {
                    Mutability::Not
                };
                let pointee = match self.shallow(expected).kind() {
                    TyKind::Ref(found, pointee) if found == m => pointee,
                    TyKind::Ref(..) => {
                        self.error(range, "mismatched types: types differ in mutability");
                        Ty::ERROR
                    }
                    TyKind::Infer(var) if !self.unknown(var).0 => {
                        let pointee = self.new_var(false, range);
                        self.unify(expected, Ty::new(TyKind::Ref(m, pointee)));
                        pointee
                    }
                    TyKind::Error => Ty::ERROR,
                    _ => {
                        let expected = self.describe(expected);
                        let found = if *mutable { "&mut _" } else { "&_" };
                        self.error(
                            range,
                            format!("mismatched types: expected `{expected}`, found `{found}`"),
                        );
                        Ty::ERROR
                    }
                };
                self.check_pat_in(*inner, pointee, BindingMode::Move, uninit);
            }
            Pat::Or(alternatives) => {
                for &alt in alternatives.iter() {
                    self.check_pat_in(alt, expected, mode, uninit);
                }
            }
            Pat::Expr(expr) => {
                if let Expr::Unary { .. } = body.exprs[*expr] {
                    self.negated_pats.insert(*expr);
                }
                if let Expr::Literal(Literal::ByteStr(_) | Literal::CStr(_)) = body.exprs[*expr] {
                    self.error(
                        range,
                        "byte string literals in patterns are not supported yet",
                    );
                }
                // A string is compared with `<str as PartialEq>::eq`, a
                // call like an operator's.
                if let Expr::Literal(Literal::Str(_)) = body.exprs[*expr]
                    && let Some((_, eq)) = self.lang_fn(BinaryOp::Cmp(CmpOp::Eq))
                {
                    self.overloads.insert(*expr, (eq, vec![Ty::STR, Ty::STR]));
                }
                if let Expr::Path(_) = body.exprs[*expr] {
                    self.const_pats.insert(*expr);
                    self.deferred.push(Deferred::ConstPat {
                        expr: *expr,
                        in_range: false,
                    });
                }
                let ty = self.infer_expr(*expr);
                if !self.unify(expected, ty) {
                    self.mismatch(expected, ty, range);
                }
            }
            Pat::Range { start, end, .. } => {
                for &end in start.iter().chain(end) {
                    if let Expr::Path(_) = body.exprs[end] {
                        self.const_pats.insert(end);
                        self.deferred.push(Deferred::ConstPat {
                            expr: end,
                            in_range: true,
                        });
                    }
                    let ty = self.infer_expr(end);
                    if !self.unify(expected, ty) {
                        self.mismatch(expected, ty, self.range(end));
                    }
                }
                self.deferred.push(Deferred::RangePat { pat, ty: expected });
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
                self.check_tuple_pats(pats, *rest, elements, mode, uninit);
            }
            Pat::TupleStruct {
                strukt,
                variant,
                pats,
                rest,
            } => {
                let generics = body.pat_generic_args.get(pat);
                let ty = self.instantiate_adt(*strukt, generics, range);
                if !self.unify(ty, expected) {
                    self.mismatch(expected, ty, range);
                }
                let fields = self.variant_fields_of(ty, *variant, range);
                self.register_struct_bounds(ty, &[], range);
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
                    let what = if items.structs[*strukt].is_enum() {
                        "tuple variant"
                    } else {
                        "tuple struct"
                    };
                    self.error(
                        range,
                        format!(
                            "this pattern has {got} field{}, but the corresponding {what} has {want} field{}",
                            plural(got),
                            plural(want)
                        ),
                    );
                }
                self.check_tuple_pats(pats, *rest, fits.then_some(fields), mode, uninit);
            }
            Pat::Record {
                strukt,
                variant,
                fields,
                rest,
            } => {
                let generics = body.pat_generic_args.get(pat);
                let ty = self.instantiate_adt(*strukt, generics, range);
                self.register_struct_bounds(ty, &[], range);
                if !self.unify(ty, expected) {
                    self.mismatch(expected, ty, range);
                }
                let field_types = self.variant_fields_of(ty, *variant, range);
                let def = items.structs[*strukt].variant(*variant);
                let names = self.source_map.pat_fields.get(pat);
                let mut private = Vec::new();
                let mut mentioned = Vec::new();
                let mut unknown = false;
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
                        Some((index, _)) => field_types.get(index).copied().unwrap_or(Ty::ERROR),
                        None => {
                            let message = format!(
                                "{} `{}` does not have a field named `{name}`",
                                def.what(),
                                def.path()
                            );
                            self.error(name_range, message);
                            unknown = true;
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
                    self.check_pat_in(field_pat, field_ty, mode, uninit);
                }
                self.private_fields(def.name, &private);
                // rustc reports unknown fields instead of missing ones.
                if !rest && !unknown {
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
            Pat::Unit(strukt, _) => {
                let generics = body.pat_generic_args.get(pat);
                let ty = self.instantiate_adt(*strukt, generics, range);
                if !self.unify(ty, expected) {
                    self.mismatch(expected, ty, range);
                }
            }
        }
    }

    /// `expected` without the references a pattern sees through, and the
    /// binding mode that leaves: by reference, mutable only if every
    /// reference was.
    fn peel_refs(
        &mut self,
        pat: PatId,
        mut expected: Ty,
        mut mode: BindingMode,
    ) -> (Ty, BindingMode) {
        let mut derefs = 0;
        while let TyKind::Ref(m, pointee) = self.shallow(expected).kind() {
            expected = pointee;
            derefs += 1;
            mode = match mode {
                BindingMode::Ref(Mutability::Not) => BindingMode::Ref(Mutability::Not),
                _ => BindingMode::Ref(m),
            };
        }
        if derefs > 0 {
            self.pat_derefs.insert(pat, derefs);
        }
        (expected, mode)
    }

    /// The fields of a tuple or tuple struct pattern against the types of
    /// `elements`, if they fit.
    fn check_tuple_pats(
        &mut self,
        pats: &[PatId],
        rest: Option<u32>,
        elements: Option<&[Ty]>,
        mode: BindingMode,
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
            self.check_pat_in(pat, ty, mode, uninit);
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
            Expr::Literal(Literal::Char(_)) => Ty::CHAR,
            Expr::Literal(Literal::Str(_)) => Ty::new(TyKind::Ref(Mutability::Not, Ty::STR)),
            Expr::Literal(Literal::ByteStr(bytes)) => Ty::new(TyKind::Ref(
                Mutability::Not,
                Ty::array(Ty::int(IntTy::U8), bytes.len() as u64),
            )),
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
                match self.resolve_assoc(id, *ty, *trait_, *name) {
                    Some((AssocItem::Const(c), target, trait_args)) => {
                        self.instantiate_const(id, c, target, trait_args.as_deref())
                    }
                    Some((AssocItem::Function(_), target, _)) if self.const_pats.contains(&id) => {
                        let path = match (target, trait_) {
                            (Some(target), Some(t)) if ty.is_some() => {
                                format!("<{target} as {}>::{name}", self.items.traits[*t].name)
                            }
                            (Some(target), _) => format!("{target}::{name}"),
                            (None, _) => name.to_string(),
                        };
                        self.error(
                            self.range(id),
                            format!(
                                "expected unit struct, unit variant or constant, found associated function `{path}`"
                            ),
                        );
                        Ty::ERROR
                    }
                    Some((AssocItem::Function(f), target, trait_args)) => {
                        self.fn_item(id, f, target, trait_args)
                    }
                    None => Ty::ERROR,
                }
            }
            Expr::Path(Res::Function(f)) => match self.select_path_fn(id, *f) {
                Some((f, target, trait_args)) => self.fn_item(id, f, target, trait_args),
                None => Ty::ERROR,
            },
            Expr::Path(Res::Struct(strukt)) => match self.items.structs[*strukt].kind {
                StructKind::Unit => self.instantiate_struct(*strukt, Some(id), self.range(id)),
                _ => {
                    self.error(
                        self.range(id),
                        "tuple struct constructors as values are not supported yet",
                    );
                    Ty::ERROR
                }
            },
            Expr::Path(Res::Variant(strukt, variant)) => {
                match self.items.structs[*strukt].variant(*variant).kind {
                    StructKind::Unit => {
                        let ty = self.instantiate_struct(*strukt, Some(id), self.range(id));
                        self.meet_expectation(id, ty);
                        ty
                    }
                    _ => {
                        self.error(
                            self.range(id),
                            "tuple variant constructors as values are not supported yet",
                        );
                        Ty::ERROR
                    }
                }
            }
            Expr::Path(Res::Const(c)) => self.path_const(id, *c),
            Expr::Path(Res::Static(c)) => {
                if self.items.consts[*c].is_static_mut() && self.unsafe_depth == 0 {
                    self.error(
                        self.range(id),
                        "use of mutable static is unsafe and requires unsafe function or block",
                    );
                }
                self.sigs.consts.get(*c).copied().unwrap_or(Ty::ERROR)
            }
            Expr::Path(Res::Err) => Ty::ERROR,
            Expr::Tuple(fields) => {
                let elements: Vec<Ty> = fields.iter().map(|&f| self.infer_expr(f)).collect();
                Ty::tuple(&elements)
            }
            Expr::Field { expr, name } => self.infer_field(id, *expr, *name),
            Expr::Array(elements) => {
                let elem = match self.expected_elem(id) {
                    Some(elem) => elem,
                    None => {
                        // Function items of different functions coerce to
                        // their pointer type, as in rustc.
                        let types: Vec<(ExprId, Ty)> =
                            elements.iter().map(|&e| (e, self.infer_expr(e))).collect();
                        if let Some(ptr) = self.fn_lub(&types) {
                            return Ty::array(ptr, elements.len() as u64);
                        }
                        let elem = self.new_var(false, self.range(id));
                        for (element, ty) in types {
                            self.coerce(ty, elem, element);
                        }
                        return Ty::array(elem, elements.len() as u64);
                    }
                };
                for &element in elements.iter() {
                    self.expect(element, elem);
                    let ty = self.infer_expr(element);
                    self.coerce(ty, elem, element);
                }
                Ty::array(elem, elements.len() as u64)
            }
            Expr::Repeat { value, count } => {
                let elem = self.expected_elem(id);
                if let Some(elem) = elem {
                    self.expect(*value, elem);
                }
                let ty = self.infer_expr(*value);
                let elem = match elem {
                    Some(elem) => {
                        self.coerce(ty, elem, *value);
                        elem
                    }
                    None => ty,
                };
                let mut errors = Vec::new();
                let scope = TypeScope {
                    lens: Some(&self.lens),
                    ..TypeScope::default()
                };
                let len = crate::array_len(self.items, self.module, count, scope, &mut errors);
                for (range, message) in errors {
                    self.error(range, message);
                }
                let Some(len) = len else {
                    return Ty::ERROR;
                };
                // A constant is made anew for each element.
                let constant = matches!(
                    body.exprs[*value],
                    Expr::Path(Res::Const(_) | Res::Assoc { .. })
                );
                if len > 1
                    && !constant
                    && let Some(&copy) = self.items.lang_traits.get(&flux_intern::sym::copy)
                {
                    let trait_ref = crate::TraitRef {
                        trait_: copy,
                        name: self.items.traits[copy].name,
                        args: Ty::list(&[]),
                    };
                    let range = self.range(*value);
                    self.register(
                        trait_ref.with_self(elem),
                        range,
                        solve::ObligationKind::Bound,
                    );
                }
                Ty::array(elem, len)
            }
            Expr::Index { base, index } => self.infer_index(id, *base, *index),
            Expr::Record {
                strukt,
                variant,
                fields,
                spread,
            } => self.infer_record(id, *strukt, *variant, fields, *spread),
            Expr::Let { pat, expr } => {
                let ty = self.infer_expr(*expr);
                self.check_pat(*pat, ty, false);
                Ty::BOOL
            }
            Expr::Match { scrutinee, arms } => self.infer_match(id, *scrutinee, arms),
            Expr::Block {
                stmts,
                tail,
                is_unsafe,
            } => {
                // A block with items resolves names in its own module.
                let outer = self.module;
                if let Some(&module) = body.block_modules.get(id) {
                    self.module = module;
                }
                self.unsafe_depth += u32::from(*is_unsafe);
                let ty = self.infer_block(stmts, *tail);
                self.unsafe_depth -= u32::from(*is_unsafe);
                self.module = outer;
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
                        if let Some(ptr) =
                            self.fn_lub(&[(*then_branch, then_ty), (*else_branch, else_ty)])
                        {
                            return ptr;
                        }
                        self.join(then_ty, else_ty, id)
                    }
                }
            }
            Expr::Loop {
                body: loop_body,
                source,
            } => {
                let ty = match source {
                    LoopSource::While | LoopSource::For => Ty::UNIT,
                    LoopSource::Loop => self.new_var(false, self.range(id)),
                };
                // A `for` loop's body must be `()`, where it is written.
                if let Some((inner, _)) = self.body.for_loop_match(id) {
                    self.unit_matches.insert(inner);
                }
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
                    LoopSource::While | LoopSource::For => Ty::UNIT,
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
                    } else if state.source == LoopSource::For && value.is_some() {
                        self.error(self.range(id), "`break` with value from a `for` loop");
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
                if self.in_static_mut(*expr) {
                    self.static_mut_ref(id, *mutable);
                }
                if *mutable {
                    self.mutable_place(*expr, false, self.range(id));
                }
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
                self.deferred.push(Deferred::Unary {
                    expr: id,
                    op: *op,
                    ty,
                });
                ty
            }
            Expr::Binary { op, lhs, rhs } => self.infer_binary(id, *op, *lhs, *rhs),
            Expr::Try { expr } => self.infer_try(id, *expr),
            Expr::Closure {
                params,
                param_types,
                ret,
                body: closure_body,
                is_move,
            } => self.infer_closure(id, params, param_types, *ret, *closure_body, *is_move),
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
                // Only `u8` casts to `char`, so a literal is one: `65 as char`.
                if bare_literal && to == Ty::CHAR {
                    self.unify(from, Ty::int(IntTy::U8));
                }
                // A cast to a type that is not a scalar is a coercion, which
                // may fix what is not known yet: `None as Option<i32>`.
                if matches!(to.kind(), TyKind::Adt(..) | TyKind::Tuple(_))
                    && self.can_unify(from, to)
                {
                    self.unify(from, to);
                }
                // `r as *const T`: the coercion of a reference to a raw
                // pointer, which reborrows a `&mut`, then maybe a cast of
                // the pointer. A reference to an array may become a
                // pointer to its first element.
                if let (TyKind::Ref(m, pointee), TyKind::RawPtr(to_m, target)) =
                    (self.shallow(from).kind(), self.shallow(to).kind())
                {
                    let target = self.shallow(target);
                    let elem = match self.shallow(pointee).kind() {
                        TyKind::Array(elem, _) => Some(elem),
                        _ => None,
                    };
                    if self.can_unify(pointee, target) {
                        self.unify(pointee, target);
                    } else if let Some(elem) = elem
                        && self.can_unify(elem, target)
                    {
                        self.unify(elem, target);
                    }
                    if m == Mutability::Mut {
                        self.reborrows.insert(*expr, to_m);
                    }
                }
                // `f as fn(i32) -> i32`: a coercion.
                if let TyKind::FnPtr(_) = to.kind() {
                    self.coerce(from, to, *expr);
                    return to;
                }
                self.deferred.push(Deferred::Cast { expr: id, from, to });
                to
            }
        }
    }

    /// `expr?`: the `Some` or `Ok` value of `expr`, an `Option` or a
    /// `Result`, whose `None` or `Err` the function returns, and so must
    /// return an `Option` or a `Result` too. The error of a `Result` is
    /// converted to the function's with `From::from`, as rustc's
    /// `FromResidual` impl for `Result` does. rustc's errors are those
    /// of the `Try` and `FromResidual` bounds its desugaring has.
    fn infer_try(&mut self, id: ExprId, operand: ExprId) -> Ty {
        let ty = self.infer_expr(operand);
        let ty = self.shallow(ty);
        let whole = self.range(id);
        let question = self.source_map.ops.get(id).copied().unwrap_or(whole);
        let lang = |name: &str| self.items.lang_enums.get(&Symbol::intern(name)).copied();
        let (option, result) = (lang("Option"), lang("Result"));
        let arg = |args: &[Ty], i: usize| args.get(i).copied().unwrap_or(Ty::ERROR);
        let (output, error) = match ty.kind() {
            TyKind::Adt(adt, args) if Some(adt.strukt) == option => (arg(args, 0), None),
            TyKind::Adt(adt, args) if Some(adt.strukt) == result => {
                (arg(args, 0), Some(arg(args, 1)))
            }
            TyKind::Error => return Ty::ERROR,
            TyKind::Infer(var) if !self.unknown(var).0 => {
                self.annotations_needed(operand, var, self.range(operand));
                return Ty::ERROR;
            }
            _ => {
                self.error(
                    whole,
                    "the `?` operator can only be applied to values that implement `Try`",
                );
                return Ty::ERROR;
            }
        };
        match self.const_context {
            // Its `return` is not allowed where there is no function.
            Some("constants") => {
                self.error(question, "return statement outside of function body");
                return output;
            }
            Some(context) => {
                let ty = self.describe(ty);
                self.error(whole, format!("`?` is not allowed on `{ty}` in {context}"));
                self.error(whole, "`Try` is not yet stable as a const trait");
                self.error(whole, "`FromResidual` is not yet stable as a const trait");
            }
            None => {}
        }
        let ret = self.shallow(self.ret);
        let ret_adt = match ret.kind() {
            TyKind::Error => return output,
            TyKind::Adt(adt, args) => Some((adt.strukt, args)),
            _ => None,
        };
        match (error, ret_adt) {
            (None, Some((s, _))) if Some(s) == option => {}
            (Some(error), Some((s, args))) if Some(s) == result => {
                let target = arg(args, 1);
                let from = self.items.lang_traits.get(&Symbol::intern("From")).copied();
                let function = from.and_then(|t| {
                    let f = self.items.traits[t].function(self.items, Symbol::intern("from"))?;
                    Some((t, f))
                });
                if let Some((t, function)) = function {
                    let trait_ref = crate::TraitRef {
                        trait_: t,
                        name: self.items.traits[t].name,
                        args: Ty::list(&[error]),
                    };
                    self.register(
                        trait_ref.with_self(target),
                        question,
                        solve::ObligationKind::QuestionMark { target },
                    );
                    self.try_conversions.insert(id, (function, vec![target, error]));
                    self.select_obligations();
                }
            }
            (None, Some((s, _))) if Some(s) == result => self.error(
                question,
                "the `?` operator can only be used on `Result`s, not `Option`s, in a function that returns `Result`",
            ),
            (Some(_), Some((s, _))) if Some(s) == option => self.error(
                question,
                "the `?` operator can only be used on `Option`s, not `Result`s, in a function that returns `Option`",
            ),
            _ => self.error(
                question,
                "the `?` operator can only be used in a function that returns `Result` or `Option` (or another type that implements `FromResidual`)",
            ),
        }
        output
    }

    /// `match scrutinee { arms }`: the arms' common type, where `!` arms
    /// take the others'.
    fn infer_match(&mut self, id: ExprId, scrutinee: ExprId, arms: &[flux_hir::MatchArm]) -> Ty {
        if let [arm] = arms
            && self.body.for_loop_match(arm.body).is_some()
        {
            self.for_heads.insert(scrutinee);
        }
        let ty = self.infer_expr(scrutinee);
        let expected = self.expected.get(id).copied();
        let mut result: Option<Ty> = None;
        let mut values = Vec::new();
        for arm in arms {
            self.check_pat(arm.pat, ty, false);
            if let Some(guard) = arm.guard {
                let guard_ty = self.infer_expr(guard);
                self.coerce(guard_ty, Ty::BOOL, guard);
            }
            if let Some(expected) = expected {
                self.expect(arm.body, expected);
            }
            let arm_ty = self.infer_expr(arm.body);
            if self.unit_matches.contains(&id) {
                self.coerce(arm_ty, Ty::UNIT, arm.body);
                result = Some(Ty::UNIT);
                continue;
            }
            if self.shallow(arm_ty) == Ty::NEVER {
                continue;
            }
            values.push((arm.body, arm_ty));
            if values.len() > 1
                && let Some(ptr) = self.fn_lub(&values)
            {
                result = Some(ptr);
                continue;
            }
            match result {
                None => result = Some(arm_ty),
                Some(first) => {
                    if !self.unify(first, arm_ty) {
                        let (a, b) = (self.describe(first), self.describe(arm_ty));
                        self.error(
                            self.range(arm.body),
                            format!(
                                "`match` arms have incompatible types: expected `{a}`, found `{b}`"
                            ),
                        );
                    }
                }
            }
        }
        result.unwrap_or(Ty::NEVER)
    }

    fn infer_block(&mut self, stmts: &[Stmt], tail: Option<ExprId>) -> Ty {
        let mut diverges = false;
        for stmt in stmts {
            match *stmt {
                Stmt::Let {
                    pat,
                    ty,
                    init,
                    else_branch,
                } => {
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
                    if let Some(else_branch) = else_branch {
                        let else_ty = self.infer_expr(else_branch);
                        if !matches!(self.shallow(else_ty).kind(), TyKind::Never | TyKind::Error) {
                            let found = self.describe(else_ty);
                            self.error(
                                self.range(else_branch),
                                format!("`else` clause of `let...else` does not diverge: expected `!`, found `{found}`"),
                            );
                        }
                    }
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

    /// Where `values` are function items of different functions, their
    /// common function pointer type, which each coerces to (rustc's
    /// least upper bound); `None` otherwise.
    fn fn_lub(&mut self, values: &[(ExprId, Ty)]) -> Option<Ty> {
        let mut defs = Vec::new();
        for &(expr, ty) in values {
            match self.shallow(ty).kind() {
                TyKind::FnDef(def) => defs.push((expr, def)),
                TyKind::Never => {}
                _ => return None,
            }
        }
        let (_, first) = *defs.first()?;
        if defs.iter().all(|(_, d)| d.function == first.function) {
            return None;
        }
        let is_unsafe = defs.iter().any(|(_, d)| d.sig.is_unsafe);
        let ptr = Ty::new(TyKind::FnPtr(crate::FnSigTy {
            is_unsafe,
            ..first.sig
        }));
        for &(expr, def) in &defs {
            let ty = Ty::new(TyKind::FnDef(def));
            self.coerce(ty, ptr, expr);
        }
        Some(ptr)
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
                match self.resolve_assoc(callee, ty, trait_, name) {
                    Some((AssocItem::Function(f), target, trait_args)) => {
                        Some((f, target, trait_args))
                    }
                    Some((AssocItem::Const(c), target, trait_args)) => {
                        let ty = self.instantiate_const(callee, c, target, trait_args.as_deref());
                        self.not_callable(callee, ty);
                        None
                    }
                    None => None,
                }
            }
            Expr::Path(Res::Struct(strukt)) => {
                return self.infer_constructor(id, callee, strukt, 0, args);
            }
            Expr::Path(Res::Variant(strukt, variant)) => {
                return self.infer_constructor(id, callee, strukt, variant, args);
            }
            Expr::Path(Res::Err) | Expr::Missing => None,
            _ => {
                let ty = self.infer_expr(callee);
                match self.shallow(ty).kind() {
                    TyKind::FnDef(def) => {
                        return self.call_value(id, callee, def.sig, Some(def.function), args);
                    }
                    TyKind::FnPtr(sig) => return self.call_value(id, callee, sig, None, args),
                    TyKind::Infer(var) if !self.unknown(var).0 => {
                        self.annotations_needed(callee, var, self.range(callee));
                    }
                    _ => return self.infer_value_call(id, callee, ty, args),
                }
                None
            }
        };
        let Some((function, target, trait_args)) = found else {
            self.infer_args(args);
            return Ty::ERROR;
        };
        let data = &self.items.functions[function];
        if crate::traits::is_destructor(self.items, function) {
            self.error(self.range(callee), "explicit use of destructor method");
        }
        self.check_fn_trait_method(function, self.range(callee));
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
        // What the function's own arguments make of its return type must
        // meet its structs' bounds; its block's were checked with it.
        let first_own = crate::traits::owner_param_count(self.items, data.owner) as u32;
        let ret = self.sigs.fns[function].ret;
        if (first_own..first_own + data.generics.len() as u32)
            .any(|i| crate::mentions_param(ret, i))
        {
            self.register_wf(sig.ret, self.range(id), false);
        }
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
        let at = self.source_map.segments.get(callee).copied();
        let at = at.unwrap_or_else(|| self.range(callee));
        self.hint_closure_args(function, &generic_args, &sig.params, args, at);
        // Closures last, as rustc checks them, so that the other
        // arguments may tell their parameters' types.
        for closures in [false, true] {
            for (i, &arg) in args.iter().enumerate() {
                if self.is_closure(arg) != closures {
                    continue;
                }
                if let Some(&param) = sig.params.get(i) {
                    self.expect(arg, param);
                }
                let mut ty = self.infer_expr(arg);
                // A `for` loop over a reference to an array is one over a
                // slice of it, which `core` makes iterable, as rustc's
                // `impl IntoIterator for &[T; N]` gives the slice's
                // iterator; arrays themselves are by the compiler's.
                if self.for_heads.contains(&id)
                    && let TyKind::Ref(m, pointee) = self.shallow(ty).kind()
                    && let TyKind::Array(elem, _) = self.shallow(pointee).kind()
                {
                    let slice = Ty::new(TyKind::Ref(m, Ty::slice(elem)));
                    self.coerce(ty, slice, arg);
                    ty = slice;
                }
                match sig.params.get(i) {
                    Some(&param) => self.coerce(ty, param, arg),
                    None if data.is_variadic => {
                        self.deferred.push(Deferred::VariadicArg { expr: arg, ty });
                    }
                    None => {}
                }
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
                format!("call to unsafe function `{name}` is unsafe and requires unsafe function or block"),
            );
        }
        self.check_signature_privacy(sig, id);
        let of_trait = matches!(data.owner, Some(flux_hir::Owner::Trait(_)));
        if let Some(context) = self.const_context
            && (data.const_unstable || of_trait)
        {
            self.deferred.push(Deferred::ConstUnstable {
                function,
                args: Ty::list(&generic_args),
                call: self.range(id),
                whole: self.range(id),
                context,
            });
        } else if let Some(context) = self.const_context
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

    /// The type of a path to `function`, used as a value rather than
    /// called: its function item type, with the generic arguments the
    /// path gives or new variables, whose bounds must hold, as at a call.
    fn fn_item(
        &mut self,
        expr: ExprId,
        function: FunctionId,
        target: Option<Ty>,
        trait_args: Option<Vec<Ty>>,
    ) -> Ty {
        let items = self.items;
        let data = &items.functions[function];
        if data.is_variadic {
            self.error(
                self.range(expr),
                "variadic functions as values are not supported yet",
            );
            return Ty::ERROR;
        }
        if crate::traits::is_destructor(items, function) {
            self.error(self.range(expr), "explicit use of destructor method");
        }
        let range = self.range(expr);
        let args = self.instantiate_fn(expr, function, target, trait_args.as_deref(), range);
        let sig = self.instance_sig(function, &args).into_owned();
        let segment = self.source_map.segments.get(expr).copied();
        let site = CallSite {
            callee: segment.unwrap_or(range),
            whole: range,
            receiver: None,
            args: &[],
            qself: self.qself_range(expr),
            expr,
        };
        let known = match self.body.exprs[expr] {
            Expr::Path(Res::Assoc { ty: Some(_), .. }) => target,
            _ => target.filter(|_| {
                self.body
                    .generic_args
                    .get(expr)
                    .is_some_and(|g| g.ty.is_some())
            }),
        };
        self.register_call_bounds(function, &args, site, known);
        self.check_signature_privacy(&sig, expr);
        let name = Symbol::intern(&items.function_path(data));
        Ty::new(TyKind::FnDef(crate::FnDefTy {
            function,
            name,
            args: Ty::list(&args),
            sig: crate::FnSigTy::new(&sig.params, sig.ret, data.is_unsafe),
        }))
    }

    /// A call `id` of `callee`, a value of a function item's type (whose
    /// function is `function`) or a function pointer's, with signature
    /// `sig`.
    fn call_value(
        &mut self,
        id: ExprId,
        callee: ExprId,
        sig: crate::FnSigTy,
        function: Option<FunctionId>,
        args: &[ExprId],
    ) -> Ty {
        let params = sig.params();
        let (want, got) = (params.len(), args.len());
        if want != got {
            let verb = if got == 1 { "was" } else { "were" };
            self.error(
                self.range(callee),
                format!(
                    "this function takes {want} argument{} but {got} argument{} {verb} supplied",
                    plural(want),
                    plural(got)
                ),
            );
        }
        for (i, &arg) in args.iter().enumerate() {
            if let Some(&param) = params.get(i) {
                self.expect(arg, param);
            }
            let ty = self.infer_expr(arg);
            if let Some(&param) = params.get(i) {
                self.coerce(ty, param, arg);
            }
        }
        if sig.is_unsafe && self.unsafe_depth == 0 {
            let message = match function {
                Some(f) => format!(
                    "call to unsafe function `{}` is unsafe and requires unsafe function or block",
                    function_name(self.items, f)
                ),
                None => "call to unsafe function is unsafe and requires unsafe function or block"
                    .to_owned(),
            };
            self.error(self.range(id), message);
        }
        if let Some(context) = self.const_context {
            match function {
                Some(f) if !self.items.functions[f].is_const => {
                    let data = &self.items.functions[f];
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
                Some(_) => {}
                None => self.error(
                    self.range(id),
                    format!("function pointer calls are not allowed in {context}"),
                ),
            }
        }
        sig.ret()
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
                let message = self.not_found(target, data.name);
                self.error(segment, message);
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
                TyKind::Ref(_, t)
                | TyKind::RawPtr(_, t)
                | TyKind::Array(t, _)
                | TyKind::Slice(t) => structs(t, out),
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

    /// `P(a, b)` or `E::V(a, b)`, a tuple struct's or variant's
    /// constructor.
    fn infer_constructor(
        &mut self,
        call: ExprId,
        callee: ExprId,
        strukt: StructId,
        variant: u32,
        args: &[ExprId],
    ) -> Ty {
        let def = self.items.structs[strukt].variant(variant);
        if def.kind != StructKind::Tuple {
            let message = match def.enum_name {
                Some(_) => format!("expected function, found enum variant `{}`", def.path()),
                None => format!("expected function, found struct `{}`", def.name),
            };
            self.error(self.range(callee), message);
            self.infer_args(args);
            return Ty::ERROR;
        }
        let ty = self.instantiate_struct(strukt, Some(callee), self.range(callee));
        self.meet_expectation(call, ty);
        let fields = self.variant_fields_of(ty, variant, self.range(callee));
        if fields.len() != args.len() {
            let (want, got) = (fields.len(), args.len());
            let verb = if got == 1 { "was" } else { "were" };
            let what = if def.enum_name.is_some() {
                "enum variant"
            } else {
                "struct"
            };
            self.error(
                self.range(callee),
                format!(
                    "this {what} takes {want} argument{} but {got} argument{} {verb} supplied",
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
        // As rustc, at the argument, then the whole call.
        let values: Vec<Option<ExprId>> = args.iter().map(|&a| Some(a)).collect();
        let values = self.variant_values(strukt, variant, values);
        self.register_struct_bounds(ty, &values, self.range(callee));
        self.register_wf(ty, self.range(call), false);
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

    /// The element type the context wants of an array expression, if it
    /// wants an array.
    fn expected_elem(&mut self, expr: ExprId) -> Option<Ty> {
        let expected = self.expected.get(expr).copied()?;
        match self.shallow(expected).kind() {
            TyKind::Array(elem, _) => Some(elem),
            _ => None,
        }
    }

    /// `base[index]`: an element of an array or slice, which `base` may
    /// be behind references, at a `usize` index.
    /// `base[index]`: built in for arrays and slices indexed by a
    /// `usize`, otherwise `*Index::index(&base, index)`, found as rustc's
    /// `lookup_indexing` finds it: on each type `base` dereferences to,
    /// then on the slice an array unsizes to. Which of `index` and
    /// `index_mut` is called is settled once it is known whether the
    /// place is used mutably ([`Self::mutable_place`]).
    fn infer_index(&mut self, id: ExprId, base: ExprId, index: ExprId) -> Ty {
        let ty = self.infer_expr(base);
        let index_ty = self.infer_expr(index);
        // rustc points at `[index]`.
        let brackets = TextRange::new(self.range(base).end(), self.range(id).end());
        let usize = Ty::int(IntTy::Usize);
        let index_ty = self.shallow(index_ty);
        let integral = match index_ty.kind() {
            TyKind::Infer(var) => self.unknown(var).0,
            TyKind::Error | TyKind::Never => true,
            _ => index_ty == usize,
        };
        let mut step = self.shallow(ty);
        let mut derefs = 0;
        loop {
            match step.kind() {
                TyKind::Array(elem, _) | TyKind::Slice(elem) if integral => {
                    if derefs > 0 {
                        self.field_derefs.insert(id, derefs);
                    }
                    self.unify(index_ty, usize);
                    return elem;
                }
                TyKind::Error => return Ty::ERROR,
                TyKind::Infer(var) if !self.unknown(var).0 => {
                    self.annotations_needed(base, var, self.range(base));
                    return Ty::ERROR;
                }
                _ => {}
            }
            if let Some(output) = self.index_overload(id, step, index_ty, derefs, false) {
                return output;
            }
            match step.kind() {
                TyKind::Ref(_, pointee) => {
                    step = self.shallow(pointee);
                    derefs += 1;
                }
                _ => break,
            }
        }
        if let TyKind::Array(elem, _) = step.kind()
            && let Some(output) = self.index_overload(id, Ty::slice(elem), index_ty, derefs, true)
        {
            return output;
        }
        let mut what = self.describe(index_ty);
        // rustc names by their path the ranges that `core::range` has new
        // versions of.
        if let TyKind::Adt(adt, _) = index_ty.kind()
            && self.items.lang_enums.get(&adt.name) == Some(&adt.strukt)
            && matches!(adt.name.as_str(), "Range" | "RangeFrom" | "RangeInclusive")
        {
            what = format!("std::ops::{what}");
        }
        match step.kind() {
            TyKind::Array(elem, _) | TyKind::Slice(elem) => {
                let slice = self.describe(Ty::slice(elem));
                self.error(
                    self.range(index),
                    format!("the type `{slice}` cannot be indexed by `{what}`"),
                );
                elem
            }
            // A `str` is indexed only by ranges of `usize`, as rustc's
            // `Index` impls for it say.
            TyKind::Str => {
                self.error(
                    self.range(index),
                    format!("the type `str` cannot be indexed by `{what}`"),
                );
                Ty::ERROR
            }
            _ => {
                let ty = self.describe(ty);
                self.error(
                    brackets,
                    format!("cannot index into a value of type `{ty}`"),
                );
                Ty::ERROR
            }
        }
    }

    /// `base[index]` as `*Index::index(&base, index)` on `ty`, which
    /// `base` dereferences to `derefs` times (then unsized, if
    /// `unsize`), if `ty` may implement `Index<I>`: its `Output`.
    fn index_overload(
        &mut self,
        id: ExprId,
        ty: Ty,
        index_ty: Ty,
        derefs: u32,
        unsize: bool,
    ) -> Option<Ty> {
        let items = self.items;
        let t = *items.lang_traits.get(&Symbol::intern("index"))?;
        let function = items.traits[t].function(items, Symbol::intern("index"))?;
        if !self.probe(|cx| cx.may_hold(ty, t, Some(&[index_ty]), 0)) {
            return None;
        }
        let trait_ref = crate::TraitRef {
            trait_: t,
            name: items.traits[t].name,
            args: Ty::list(&[index_ty]),
        };
        let range = self.range(id);
        self.register(trait_ref.with_self(ty), range, solve::ObligationKind::Bound);
        self.overloads.insert(id, (function, vec![ty, index_ty]));
        if derefs > 0 {
            self.field_derefs.insert(id, derefs);
        }
        if unsize {
            self.unsized_indices.insert(id, ());
        }
        self.select_obligations();
        Some(self.output(trait_ref, ty, range))
    }

    /// `expr` is a place assigned to (`assign`) or borrowed mutably, by
    /// `site`, where errors point: the
    /// overloaded indexing it goes through, without a dereference
    /// between, calls `IndexMut::index_mut`, as rustc's
    /// `convert_place_derefs_to_mutable` decides once it knows.
    fn mutable_place(&mut self, expr: ExprId, assign: bool, site: TextRange) {
        let items = self.items;
        let index = items.lang_traits.get(&Symbol::intern("index")).copied();
        let index_mut = items.lang_traits.get(&Symbol::intern("index_mut")).copied();
        let (Some(index), Some(index_mut)) = (index, index_mut) else {
            return;
        };
        let index_fn = items.traits[index].function(items, Symbol::intern("index"));
        let Some(index_mut_fn) =
            items.traits[index_mut].function(items, Symbol::intern("index_mut"))
        else {
            return;
        };
        let mut place = expr;
        loop {
            match self.body.exprs[place] {
                Expr::Field { expr: inner, .. } => {
                    if self.field_derefs.get(place).is_some() {
                        return;
                    }
                    place = inner;
                }
                Expr::Index { base, .. } => {
                    if let Some((function, args)) = self.overloads.get(place).cloned()
                        && Some(function) == index_fn
                    {
                        let (ty, index_ty) = (args[0], args[1]);
                        let range = site;
                        let known = !self.has_unknown(ty) && !self.has_unknown(index_ty);
                        if known
                            && !self.probe(|cx| cx.may_hold(ty, index_mut, Some(&[index_ty]), 0))
                        {
                            let what = self.describe(ty);
                            let message = if assign {
                                format!("cannot assign to data in an index of `{what}`")
                            } else {
                                format!("cannot borrow data in an index of `{what}` as mutable")
                            };
                            self.error(range, message);
                        } else {
                            let trait_ref = crate::TraitRef {
                                trait_: index_mut,
                                name: items.traits[index_mut].name,
                                args: Ty::list(&[index_ty]),
                            };
                            self.register(
                                trait_ref.with_self(ty),
                                range,
                                solve::ObligationKind::Bound,
                            );
                        }
                        self.overloads.insert(place, (index_mut_fn, args));
                    }
                    if self.field_derefs.get(place).is_some() {
                        return;
                    }
                    place = base;
                }
                _ => return,
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
                .map(|(i, _)| (i, sigs.field_types(adt.strukt, args)[i]))
                .map(|(i, ty)| (i, self.normalize(ty, range))),
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
        variant: u32,
        fields: &[(Symbol, ExprId)],
        spread: Option<ExprId>,
    ) -> Ty {
        let items = self.items;
        let def = items.structs[strukt].variant(variant);
        let range = self.range(id);
        let origin = self.source_map.paths.get(id).copied().unwrap_or(range);
        let ty = self.instantiate_struct(strukt, Some(id), origin);
        self.meet_expectation(id, ty);
        let field_types = self.variant_fields_of(ty, variant, range);
        let names = self.source_map.expr_fields.get(id);
        let mut seen = Vec::new();
        let mut unknown = false;
        let mut private = Vec::new();
        for (i, &(name, value)) in fields.iter().enumerate() {
            if let Some((index, _)) = def.field(name)
                && let Some(&field_ty) = field_types.get(index)
            {
                self.expect(value, field_ty);
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
                Some((index, _)) => {
                    // No types when the struct's own type is in error.
                    if let Some(&field_ty) = field_types.get(index) {
                        self.coerce(value_ty, field_ty, value);
                    }
                }
                None => {
                    unknown = true;
                    self.error(
                        name_range,
                        format!(
                            "{} `{}` has no field named `{name}`",
                            def.what(),
                            def.path()
                        ),
                    );
                }
            }
        }
        match spread {
            Some(base) if def.enum_name.is_some() => {
                self.infer_expr(base);
                self.error(
                    self.range(base),
                    "functional record update syntax requires a struct",
                );
            }
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
                        items.structs[strukt].name
                    );
                    let path = self.source_map.paths.get(id).copied();
                    self.error(path.unwrap_or(range), message);
                }
            }
        }
        self.private_fields(def.name, &private);
        let values: Vec<Option<ExprId>> = def
            .fields
            .iter()
            .map(|f| {
                fields
                    .iter()
                    .find(|(name, _)| *name == f.name)
                    .map(|&(_, v)| v)
            })
            .collect();
        let values = self.variant_values(strukt, variant, values);
        self.register_struct_bounds(ty, &values, origin);
        ty
    }

    /// "no associated function or constant named `name` found for struct
    /// `S` in the current scope", and for an enum, whose variants are
    /// looked for too, "no variant, associated function, or constant".
    pub(super) fn not_found(&mut self, target: Option<Ty>, name: Symbol) -> String {
        let is_enum = target.is_some_and(|t| {
            matches!(self.shallow(t).kind(), TyKind::Adt(adt, _) if self.sigs.is_enum(adt.strukt))
        });
        let what = target.map_or("this type".to_owned(), |t| self.describe_kind(t));
        let items = if is_enum {
            "variant, associated function, or constant"
        } else {
            "associated function or constant"
        };
        format!("no {items} named `{name}` found for {what} in the current scope")
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
        let ty = self.probe_method_call(id, receiver, method, args);
        // Closures not inferred with the method's signature, which was
        // not found.
        for &arg in args {
            if self.is_closure(arg) && self.expr_types.get(arg).is_none() {
                self.infer_expr(arg);
            }
        }
        ty
    }

    fn probe_method_call(
        &mut self,
        id: ExprId,
        receiver: ExprId,
        method: Symbol,
        args: &[ExprId],
    ) -> Ty {
        let receiver_ty = self.infer_expr(receiver);
        // Closures wait for the method's signature.
        for &arg in args {
            if !self.is_closure(arg) {
                self.infer_expr(arg);
            }
        }
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
                            (ty, d, false),
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
            if unmet.is_none()
                && (self.exists_unmet(base, method, true)
                    || self.exists_unmet_for_ref(base, method))
            {
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
        // Past the last dereference, an array unsizes to a slice, which
        // only a reference may take (rustc's probe's final step).
        if let TyKind::Array(elem, _) = base.kind() {
            let slice = Ty::new(TyKind::Slice(elem));
            for mode in [Mutability::Not, Mutability::Mut] {
                match self.probe_method(slice, Some(mode), method, inherent(slice), traits) {
                    Ok(Some((function, trait_args))) => {
                        return self.check_method(
                            id,
                            receiver,
                            function,
                            (slice, derefs, true),
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
            if unmet.is_none()
                && (self.exists_unmet(slice, method, true)
                    || self.exists_unmet_for_ref(slice, method))
            {
                unmet = Some(slice);
            }
        }
        if let TyKind::Array(..) | TyKind::Slice(_) = base.kind() {
            let builtin = match method.as_str() {
                "len" => Some((BuiltinMethod::Len, Ty::int(IntTy::Usize))),
                "is_empty" => Some((BuiltinMethod::IsEmpty, Ty::BOOL)),
                _ => None,
            };
            if let Some((builtin, ret)) = builtin {
                if !args.is_empty() {
                    let got = args.len();
                    let verb = if got == 1 { "was" } else { "were" };
                    self.error(
                        name_range,
                        format!(
                            "this method takes 0 arguments but {got} argument{} {verb} supplied",
                            plural(got)
                        ),
                    );
                }
                self.builtin_methods.insert(id, (builtin, derefs));
                return ret;
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
                    let data = &self.items.structs[adt.strukt];
                    let kind = if data.is_enum() { "enum" } else { "struct" };
                    format!("{kind} `{}{}`", adt.name, data.generics)
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
        (base, derefs, unsize): (Ty, u32, bool),
        trait_args: Option<&[Ty]>,
        args: &[ExprId],
        name_range: TextRange,
    ) -> Ty {
        let data = &self.items.functions[function];
        if crate::traits::is_destructor(self.items, function) {
            self.error(name_range, "explicit use of destructor method");
        }
        self.check_fn_trait_method(function, name_range);
        let generic_args = self.instantiate_fn(id, function, Some(base), trait_args, name_range);
        if !self.items.is_visible(data.loc.vis, self.module) {
            self.error(name_range, format!("method `{}` is private", data.name));
        }
        let sig = &self.instance_sig(function, &generic_args);
        self.check_signature_privacy(sig, id);
        let of_trait = matches!(data.owner, Some(flux_hir::Owner::Trait(_)));
        if let Some(context) = self.const_context
            && (data.const_unstable || of_trait)
        {
            self.deferred.push(Deferred::ConstUnstable {
                function,
                args: Ty::list(&generic_args),
                call: TextRange::new(name_range.start(), self.range(id).end()),
                whole: self.range(id),
                context,
            });
        } else if let Some(context) = self.const_context
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
        if autoref == Some(Mutability::Mut) && derefs == 0 {
            self.mutable_place(receiver, false, self.range(receiver));
        }
        self.methods.insert(
            id,
            Method {
                function,
                derefs,
                autoref,
                unsize,
            },
        );
        let params = &sig.params[1..];
        self.hint_closure_args(function, &generic_args, params, args, name_range);
        for (&arg, &param) in args.iter().zip(params) {
            if self.is_closure(arg) {
                self.expect(arg, param);
                self.infer_expr(arg);
            }
        }
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
                format!("call to unsafe function `{name}` is unsafe and requires unsafe function or block"),
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
                self.mutable_place(lhs, true, self.range(id));
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

    /// `lhs op rhs` where `lhs` is a struct, a generic parameter, an
    /// associated type or a reference, or an integer or `bool` with a
    /// reference on the right: a call of the method of the operator's
    /// trait (`Add::add`, `PartialEq::eq`, `AddAssign::add_assign`), whose
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
        let overloadable = matches!(
            operand.kind(),
            TyKind::Adt(..)
                | TyKind::Param(_)
                | TyKind::Alias(_)
                | TyKind::Ref(..)
                | TyKind::Array(..)
                | TyKind::Slice(_)
        );
        let op_range = self.source_map.ops.get(id).copied().unwrap_or_default();
        if let BinaryOp::Cmp(_) = op
            && let Some(message) = self.cmp_ref_error(op, operand, rhs_ty)
        {
            self.error(op_range, message);
            return Some(Ty::BOOL);
        }
        // `core` does not compare tuples or pointers yet, by reference
        // either.
        if let (BinaryOp::Cmp(_), TyKind::Ref(..)) = (op, operand.kind()) {
            let mut leaf = operand;
            while let TyKind::Ref(_, pointee) = leaf.kind() {
                leaf = self.shallow(pointee);
            }
            let what = match leaf.kind() {
                TyKind::Tuple([_, ..]) => "tuples",
                TyKind::RawPtr(..) => "pointers",
                _ => "",
            };
            if !what.is_empty() {
                self.error(
                    self.range(id),
                    format!("comparing {what} is not supported yet"),
                );
                return Some(Ty::BOOL);
            }
        }
        // `1 + &2` and `x += &1`, through `core`'s `impl Add<&i32> for
        // i32`. A comparison wants the same type on both sides.
        let primitive = matches!(operand.kind(), TyKind::Int(_) | TyKind::Bool)
            || matches!(operand.kind(), TyKind::Infer(var) if self.unknown(var).0);
        let reference_rhs = primitive
            && !matches!(op, BinaryOp::Cmp(_))
            && matches!(self.shallow(rhs_ty).kind(), TyKind::Ref(..));
        if !overloadable && !reference_rhs {
            // An integer or `bool` on the left and another type on the
            // right: no `impl` block of `core`'s takes it.
            let other = match self.shallow(rhs_ty).kind() {
                TyKind::Adt(..) | TyKind::Param(_) | TyKind::Alias(_) => true,
                TyKind::Char => operand != Ty::CHAR,
                _ => false,
            };
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
        let result = match op {
            BinaryOp::Cmp(_) => Ty::BOOL,
            BinaryOp::Assign(_) => Ty::UNIT,
            _ => Ty::ERROR,
        };
        // `&a == &b` compares `a` with `b`. `&a < &b` does too, but
        // `core`'s one `impl PartialOrd<&B> for &A` says first what `b`
        // must be, as in rustc.
        let (mut lhs_cmp, mut rhs_cmp) = (lhs, rhs_ty);
        if let (BinaryOp::Cmp(CmpOp::Eq | CmpOp::Ne), TyKind::Ref(_, a), TyKind::Ref(_, b)) =
            (op, operand.kind(), self.shallow(rhs_ty).kind())
        {
            (lhs_cmp, rhs_cmp) = (a, b);
            self.compared_refs.insert(id, ());
        }
        let Some((t, function)) = self.lang_fn(op) else {
            let (lhs, rhs) = (self.describe(lhs), self.describe(rhs_ty));
            self.error(op_range, binary_error(op, &lhs, &rhs));
            return Some(result);
        };
        // Reported if not: its result is then an error.
        let holds = self.may_implement(lhs_cmp, t);
        // rustc reports a compound assignment whose trait the left side
        // does not implement at all (E0368) at the whole expression, and
        // other unmet operators (E0369, E0277) at the operator.
        let range = match op {
            BinaryOp::Assign(_) if !holds => self.range(id),
            _ => op_range,
        };
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
            self.builtin_operands(op, lhs, (rhs, rhs_ty));
            return Some(result);
        }
        let r = self.new_var(false, self.range(rhs));
        // An array with a slice, or the reverse: the right side is what
        // it is, as the compiler's impls for them say.
        if self.array_slice_eq(lhs, rhs_ty) {
            self.unify(r, rhs_ty);
        }
        let trait_ref = crate::TraitRef {
            trait_: t,
            name: self.items.traits[t].name,
            args: Ty::list(&[r]),
        };
        // With no `impl` block for the left operand at all, rustc reports
        // the operator (E0369, E0368); with some, the unmet bound (E0277),
        // in the words of its trait's `on_unimplemented`.
        let kind = if holds {
            solve::ObligationKind::Bound
        } else {
            solve::ObligationKind::Operator {
                op: solve::Op::Binary(op),
                lhs,
                rhs: rhs_ty,
            }
        };
        self.register(trait_ref.with_self(lhs_cmp), range, kind);
        self.select_obligations();
        if matches!(self.shallow(r).kind(), TyKind::Infer(_)) {
            self.unify(r, rhs_cmp);
        } else if rhs_cmp == rhs_ty {
            self.coerce(rhs_ty, r, rhs);
        } else if !self.unify(r, rhs_cmp) {
            let range = self.range(rhs);
            self.mismatch(Ty::new(TyKind::Ref(Mutability::Not, r)), rhs_ty, range);
        }
        let mut args = vec![lhs_cmp, r];
        if let (BinaryOp::Cmp(_), TyKind::Ref(_, a), TyKind::Ref(_, b)) =
            (op, self.shallow(lhs).kind(), self.shallow(r).kind())
        {
            args = vec![a, b];
            self.compared_refs.insert(id, ());
        }
        self.overloads.insert(id, (function, args));
        self.select_obligations();
        // With the right operand's type the trait wants, as rustc does.
        let builtin = self.builtin_operands(op, lhs, (rhs, r));
        // An unmet bound (reported) has no `Output`.
        let met = holds && {
            let (lhs_cmp, r) = (self.known(lhs_cmp), self.known(r));
            self.probe(|cx| cx.may_hold(lhs_cmp, t, Some(&[r]), 0))
        };
        Some(match (op, builtin) {
            (BinaryOp::Arith(_), Some(ty)) => ty,
            (BinaryOp::Arith(_), None) if met => self.output(trait_ref, lhs, range),
            _ => result,
        })
    }

    /// rustc's `enforce_builtin_binop_types`: where the operands of an
    /// operator are, past one shared reference each, both integers (or
    /// both `bool`s for `&`, `|` and `^`, or scalars for `==` and `!=`),
    /// they must have the same type, except for shifts, as the built-in
    /// operator's do, and the result has the left one's type. A mismatch
    /// is E0308 at the right operand, besides the unmet trait bound
    /// (`&i32 + u8` is both). That type, for arithmetic.
    fn builtin_operands(
        &mut self,
        op: BinaryOp,
        lhs: Ty,
        (rhs, rhs_ty): (ExprId, Ty),
    ) -> Option<Ty> {
        let peel = |cx: &mut Self, ty: Ty| match cx.shallow(ty).kind() {
            TyKind::Ref(Mutability::Not, pointee) => cx.shallow(pointee),
            _ => cx.shallow(ty),
        };
        let (l, r) = (peel(self, lhs), peel(self, rhs_ty));
        let integral = |cx: &mut Self, ty: Ty| match ty.kind() {
            TyKind::Int(_) => true,
            TyKind::Infer(var) => cx.unknown(var).0,
            _ => false,
        };
        let both_integral = integral(self, l) && integral(self, r);
        let same = match op {
            BinaryOp::Arith(ArithOp::Shl | ArithOp::Shr)
            | BinaryOp::Assign(Some(ArithOp::Shl | ArithOp::Shr)) => {
                return both_integral.then_some(l);
            }
            BinaryOp::Arith(arith) | BinaryOp::Assign(Some(arith)) => {
                let bitwise = matches!(arith, ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor);
                both_integral || (bitwise && l == Ty::BOOL && r == Ty::BOOL)
            }
            BinaryOp::Cmp(_) => {
                let scalar = |cx: &mut Self, ty: Ty| {
                    integral(cx, ty)
                        || matches!(ty.kind(), TyKind::Bool | TyKind::Char | TyKind::RawPtr(..))
                };
                scalar(self, l) && scalar(self, r)
            }
            _ => false,
        };
        if !same {
            return None;
        }
        if !self.unify(l, r) {
            let range = self.range(rhs);
            self.mismatch(l, r, range);
        }
        Some(l)
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
    /// Whether `lhs == rhs` is one of the compiler's comparisons of an
    /// array with a slice or a reference to one, either way round.
    fn array_slice_eq(&mut self, lhs: Ty, rhs: Ty) -> bool {
        let (lhs, rhs) = (self.shallow_ref(lhs), self.shallow_ref(rhs));
        !matches!(
            (lhs.kind(), rhs.kind()),
            (TyKind::Array(..), TyKind::Array(..))
        ) && crate::traits::array_eq_elems(lhs, rhs).is_some()
    }

    fn cmp_ref_error(&mut self, op: BinaryOp, lhs: Ty, rhs: Ty) -> Option<String> {
        let rhs = self.shallow(rhs);
        let unknown = |cx: &mut Self, ty: Ty| match ty.kind() {
            TyKind::Infer(var) => Some(cx.unknown(var).0),
            _ => None,
        };
        let is_ref = |ty: Ty| matches!(ty.kind(), TyKind::Ref(..));
        let eq = *self.items.lang_traits.get(&Symbol::intern("eq"))?;
        let equality = matches!(op, BinaryOp::Cmp(CmpOp::Eq | CmpOp::Ne));
        // A reference to a slice with an array: `s == [1, 2]`.
        if self.array_slice_eq(lhs, rhs) {
            return None;
        }
        match lhs.kind() {
            TyKind::Ref(_, pointee)
                if equality && !is_ref(rhs) && unknown(self, rhs) != Some(false) =>
            {
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

    /// `-a` or `!a` where `a` is a struct, a generic parameter, an
    /// associated type or a reference: `Neg::neg(a)` or `Not::not(a)`.
    fn overload_unary(&mut self, id: ExprId, op: UnaryOp, ty: Ty) -> Option<Ty> {
        if !matches!(
            self.shallow(ty).kind(),
            TyKind::Adt(..) | TyKind::Param(_) | TyKind::Alias(_) | TyKind::Ref(..)
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
            TyKind::Ref(..)
                | TyKind::Adt(..)
                | TyKind::Closure(..)
                | TyKind::Tuple([_, ..])
                | TyKind::RawPtr(..)
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
        self.mutable_place(lhs, true, self.range(assignment));
        match self.body.exprs[lhs] {
            Expr::Field { .. }
            | Expr::Index { .. }
            | Expr::Unary {
                op: UnaryOp::Deref, ..
            } => {
                // A field or element of a local, or anything behind a
                // pointer.
                let mut base = lhs;
                loop {
                    match self.body.exprs[base] {
                        Expr::Field { expr, .. } => base = expr,
                        Expr::Index { base: inner, .. } => {
                            // Behind a pointer, or what `index_mut`
                            // returns a reference to.
                            if self.field_derefs.get(base).is_some()
                                || self.overloads.get(base).is_some()
                            {
                                return;
                            }
                            base = inner;
                        }
                        Expr::Unary {
                            op: UnaryOp::Deref, ..
                        } => return,
                        _ => break,
                    }
                }
                if !matches!(
                    self.body.exprs[base],
                    Expr::Path(Res::Local(_) | Res::Static(_) | Res::Err) | Expr::Missing
                ) {
                    self.error(
                        self.range(lhs),
                        "assigning to fields of temporaries is not supported yet",
                    );
                }
            }
            // A binding outside the closure being inferred is captured;
            // the borrow checker checks what the closure does with it.
            Expr::Path(Res::Local(binding))
                if self
                    .closure_locals
                    .last()
                    .is_some_and(|locals| !locals.contains(&binding)) => {}
            Expr::Path(Res::Local(binding)) => {
                let data = &self.body.bindings[binding];
                let first_assignment = plain && self.uninit_bindings.contains(&binding);
                if !data.mutable && !first_assignment {
                    let name = data.name;
                    let is_param = self
                        .body
                        .params
                        .iter()
                        .any(|&p| matches!(self.body.pats[p], Pat::Bind { binding: b, .. } if b == binding));
                    let message = if is_param {
                        format!("cannot assign to immutable argument `{name}`")
                    } else {
                        format!("cannot assign twice to immutable variable `{name}`")
                    };
                    self.error(self.range(assignment), message);
                }
            }
            // Checked with borrows: only a `static mut` may be assigned.
            Expr::Path(Res::Static(_) | Res::Err) | Expr::Missing => {}
            _ => self.error(self.range(lhs), "invalid left-hand side of assignment"),
        }
    }

    /// Whether `expr` is a `static mut` or a field of one.
    fn in_static_mut(&self, expr: ExprId) -> bool {
        match self.body.exprs[expr] {
            Expr::Path(Res::Static(c)) => self.items.consts[c].is_static_mut(),
            Expr::Field { expr, .. } => self.in_static_mut(expr),
            _ => false,
        }
    }

    /// Reports a reference to a `static mut` made by `expr`, as edition
    /// 2024's deny-by-default `static_mut_refs` lint does.
    fn static_mut_ref(&mut self, expr: ExprId, mutable: bool) {
        let what = if mutable { "mutable" } else { "shared" };
        self.error(
            self.range(expr),
            format!("creating a {what} reference to mutable static"),
        );
    }

    // Finishing.

    fn finish(mut self) -> InferenceResult {
        let mut reported = std::mem::take(&mut self.reported_vars);
        // What closures capture needs the types, as rustc finds it after
        // inference, integer literals' defaults included.
        if !self.closures.is_empty() {
            self.select_obligations();
            self.default_int_vars();
            self.analyze_closures();
        }
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
            self.check_sized_args(expr, function, args);
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
        self.check_const_operators(&overloads);
        let converted: Vec<(ExprId, (FunctionId, Vec<Ty>))> = self
            .try_conversions
            .iter()
            .map(|(e, call)| (e, call.clone()))
            .collect();
        let mut try_conversions = ArenaMap::new();
        for (expr, (function, args)) in converted {
            let args: Vec<Ty> = args
                .iter()
                .map(|&a| self.resolve(a, &mut reported))
                .collect();
            if args[0] != args[1] {
                let args = Ty::list(&args);
                try_conversions.insert(expr, Instance { function, args });
            }
        }
        let used: Vec<(ExprId, (ConstId, Vec<Ty>))> =
            self.consts.iter().map(|(e, c)| (e, c.clone())).collect();
        let mut consts = ArenaMap::new();
        for (expr, (konst, args)) in used {
            let args: Vec<Ty> = args
                .iter()
                .map(|&a| self.resolve(a, &mut reported))
                .collect();
            consts.insert(
                expr,
                ConstInstance {
                    konst,
                    args: Ty::list(&args),
                },
            );
        }
        let coerced: Vec<(ExprId, Ty)> = self.unsizes.iter().map(|(e, t)| (e, *t)).collect();
        let mut unsizes = ArenaMap::new();
        for (expr, ty) in coerced {
            unsizes.insert(expr, self.resolve(ty, &mut reported));
        }
        let matched: Vec<(PatId, Ty)> = self.pat_types.iter().map(|(p, t)| (p, *t)).collect();
        let mut pat_types = ArenaMap::new();
        for (pat, ty) in matched {
            pat_types.insert(pat, self.resolve(ty, &mut reported));
        }
        self.run_deferred(&mut reported);
        self.check_sized();
        let receivers: Vec<(ExprId, Mutability)> = self
            .methods
            .iter()
            .filter_map(|(e, m)| Some((e, m.autoref?)))
            .collect();
        for (call, m) in receivers {
            if let Expr::MethodCall { receiver, .. } = self.body.exprs[call]
                && self.methods[call].derefs == 0
                && self.in_static_mut(receiver)
            {
                self.static_mut_ref(call, m == Mutability::Mut);
            }
        }
        let mut deferred_pats = false;
        if self.diagnostics.is_empty() && !self.tainted {
            // Patterns that name constants are checked once their values
            // are known, which needs MIR.
            if self.const_pats.is_empty() {
                let found = crate::pats::check_body(
                    self.items,
                    self.sigs,
                    self.body,
                    self.source_map,
                    &pat_types,
                    &self.pat_derefs,
                    &self.expr_types,
                    &ArenaMap::new(),
                    &self.mode_errors,
                );
                self.diagnostics.extend(found);
            } else {
                deferred_pats = true;
                let modes = self.mode_errors.iter();
                let found = modes.map(|&(range, message)| Diagnostic::new(range, message));
                self.diagnostics.extend(found);
            }
        }
        let infos = std::mem::take(&mut self.closure_infos);
        let mut closures = ArenaMap::new();
        for (expr, info) in infos {
            let ty = self.resolve(info.ty, &mut reported);
            let captures = info
                .captures
                .iter()
                .map(|c| Capture {
                    ty: self.resolve(c.ty, &mut reported),
                    ..c.clone()
                })
                .collect();
            closures.insert(
                expr,
                ClosureInfo {
                    ty,
                    kind: info.kind,
                    captures,
                },
            );
        }
        InferenceResult {
            closures,
            expr_types: self.expr_types,
            binding_types: self.binding_types,
            field_indices: self.field_indices,
            field_derefs: self.field_derefs,
            unsized_indices: self.unsized_indices,
            methods: self.methods,
            reborrows: self.reborrows,
            deref_coercions: self.deref_coercions,
            reified: self.reified,
            calls,
            overloads,
            consts,
            compared_refs: self.compared_refs,
            try_conversions,
            pat_types,
            pat_derefs: self.pat_derefs,
            binding_refs: self.binding_refs,
            deferred_pats,
            unsizes,
            builtin_methods: self.builtin_methods,
            missing_lens: self.lens.missing.into_inner(),
            diagnostics: self.diagnostics,
        }
    }

    /// In a constant or `const fn`, operators that call trait methods:
    /// rustc's `core` implements the operator traits for the integers,
    /// `bool`, references to them, `Option` and `Result` as `const`, but
    /// unstably (E0658, and that the trait is not stable as a const
    /// trait); other types' `impl` blocks are not `const` (E0015).
    fn check_const_operators(&mut self, overloads: &ArenaMap<Expr, Instance>) {
        let Some(context) = self.const_context else {
            return;
        };
        let mut found: Vec<(ExprId, Option<Symbol>)> = overloads
            .iter()
            .map(|(expr, instance)| {
                let function = &self.items.functions[instance.function];
                let trait_name = match function.owner {
                    Some(flux_hir::Owner::Trait(t)) => self.items.traits[t].name,
                    _ => function.name,
                };
                let core = instance.args.iter().all(|&ty| self.const_operand(ty));
                (expr, core.then_some(trait_name))
            })
            .collect();
        found.sort_by_key(|&(expr, _)| expr.into_raw());
        for (expr, const_trait) in found {
            // rustc points at an index's `[index]`.
            let range = match self.body.exprs[expr] {
                Expr::Index { base, .. } => {
                    TextRange::new(self.range(base).end(), self.range(expr).end())
                }
                _ => self.range(expr),
            };
            match const_trait {
                Some(name) => {
                    self.error(
                        range,
                        format!("cannot call conditionally-const operator in {context}"),
                    );
                    self.error(
                        range,
                        format!("`{name}` is not yet stable as a const trait"),
                    );
                }
                None => self.error(
                    range,
                    format!("cannot call non-const operator in {context}"),
                ),
            }
        }
    }

    /// Whether `core`'s operator impls for `ty` are `const` in rustc's.
    fn const_operand(&self, ty: Ty) -> bool {
        match ty.kind() {
            TyKind::Int(_) | TyKind::Bool | TyKind::Tuple([]) | TyKind::Error => true,
            TyKind::Ref(_, pointee) | TyKind::Slice(pointee) => self.const_operand(pointee),
            TyKind::Adt(adt, args) => {
                let data = &self.items.structs[adt.strukt];
                self.items.is_core(data.loc.module) && args.iter().all(|&a| self.const_operand(a))
            }
            _ => false,
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
            // A closure's parameter, where rustc asks for its type.
            Expr::Path(Res::Local(binding))
                if let Some(pat) = body.exprs.values().find_map(|e| match e {
                    Expr::Closure { params, .. } => params.iter().copied().find(
                        |&p| matches!(body.pats[p], Pat::Bind { binding: b, .. } if b == binding),
                    ),
                    _ => None,
                }) =>
            {
                Some(pat)
            }
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
                    ..
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
            TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => {
                for &t in list {
                    self.unknown_vars(t, out);
                }
            }
            TyKind::Alias(alias) => {
                for &t in alias.args {
                    self.unknown_vars(t, out);
                }
            }
            TyKind::RawPtr(_, t) | TyKind::Ref(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                self.unknown_vars(t, out)
            }
            TyKind::FnDef(_) | TyKind::FnPtr(_) => {
                for t in ty.children() {
                    self.unknown_vars(t, out);
                }
            }
            _ => {}
        }
    }

    /// A generic parameter is `Sized` unless it is a trait's `Self`: a
    /// call of `function` at `expr` with an unsized argument for one is
    /// E0277, reported at the argument whose type names it, as rustc
    /// does.
    fn check_sized_args(&mut self, expr: ExprId, function: FunctionId, args: &[Ty]) {
        let trait_method = matches!(
            self.items.functions[function].owner,
            Some(flux_hir::Owner::Trait(_))
        );
        let Some((index, &arg)) = args
            .iter()
            .enumerate()
            .find(|&(i, a)| a.is_unsized() && !(trait_method && i == 0))
        else {
            return;
        };
        let body = self.body;
        let values: Vec<ExprId> = match &body.exprs[expr] {
            Expr::MethodCall { receiver, args, .. } => std::iter::once(*receiver)
                .chain(args.iter().copied())
                .collect(),
            _ => body
                .exprs
                .values()
                .find_map(|e| match e {
                    Expr::Call { callee, args } if *callee == expr => Some(args.to_vec()),
                    _ => None,
                })
                .unwrap_or_default(),
        };
        let params = &self.sigs[function].params;
        let at = params
            .iter()
            .position(|&p| crate::mentions_param(p, index as u32))
            .and_then(|i| values.get(i))
            .map_or(self.range(expr), |&v| self.range(v));
        self.error(at, crate::unsized_error(arg));
    }

    /// Values must have a size known at compile time (E0277): a slice is
    /// only a place, behind a pointer, that may be borrowed, indexed or
    /// asked its length; a binding of one is reported at its pattern, as
    /// rustc does.
    fn check_sized(&mut self) {
        let body = self.body;
        let mut places = HashSet::new();
        for (id, expr) in body.exprs.iter() {
            match *expr {
                Expr::Ref { expr, .. } | Expr::Index { base: expr, .. } => {
                    places.insert(expr);
                }
                Expr::MethodCall { receiver, .. }
                    if self.builtin_methods.get(id).is_some()
                        || self.methods.get(id).is_some_and(|m| m.autoref.is_some()) =>
                {
                    places.insert(receiver);
                }
                // Compared by reference: `a[..2] == b[..2]`.
                Expr::Binary {
                    op: BinaryOp::Cmp(_),
                    lhs,
                    rhs,
                } if self.overloads.get(id).is_some() => {
                    places.insert(lhs);
                    places.insert(rhs);
                }
                _ => {}
            }
        }
        for (pat, data) in body.pats.iter() {
            let Pat::Bind { binding, .. } = data else {
                continue;
            };
            let ty = self.binding_types.get(*binding).copied();
            if let Some(ty) = ty.filter(|t| t.is_unsized())
                && self.binding_refs.get(*binding).is_none()
            {
                let range = self.source_map.pats.get(pat).copied().unwrap_or_default();
                self.error(range, crate::unsized_error(ty));
            }
        }
        // A `let`'s value is reported with its binding.
        for expr in body.exprs.values() {
            if let Expr::Block { stmts, .. } = expr {
                for stmt in stmts.iter() {
                    if let Stmt::Let {
                        pat,
                        init: Some(init),
                        ..
                    } = *stmt
                        && matches!(body.pats[pat], Pat::Bind { .. })
                    {
                        places.insert(init);
                    }
                }
            }
        }
        let unsized_values: Vec<(ExprId, Ty)> = self
            .expr_types
            .iter()
            .filter(|&(id, ty)| ty.is_unsized() && !places.contains(&id))
            .map(|(id, ty)| (id, *ty))
            .collect();
        for (expr, ty) in unsized_values {
            self.error(self.range(expr), crate::unsized_error(ty));
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
            TyKind::FnDef(_) | TyKind::FnPtr(_) if ty.has_infer() => {
                ty.map_fn(|t| self.resolve(t, reported))
            }
            TyKind::Ref(m, pointee) => Ty::new(TyKind::Ref(m, self.resolve(pointee, reported))),
            TyKind::Array(elem, len) => Ty::array(self.resolve(elem, reported), len),
            TyKind::Slice(elem) => Ty::slice(self.resolve(elem, reported)),
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
            TyKind::Closure(id, args) if ty.has_infer() => {
                let args: Vec<Ty> = args.iter().map(|&a| self.resolve(a, reported)).collect();
                Ty::closure(id, &args)
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

    /// How rustc names `function`, of an inherent `impl` block of a
    /// generic struct or a module of `core`, with generic arguments
    /// `args` (`Option::<i32>::ok_or::<u8>`, `std::cmp::max::<i32>`), and
    /// with its block's parameters (`Option::<T>::ok_or`).
    fn unstable_paths(&self, function: FunctionId, args: &[Ty]) -> (String, String) {
        let items = self.items;
        let data = &items.functions[function];
        let imp = data.owner.and_then(flux_hir::Owner::as_impl);
        let (name, params) = match imp.map(|imp| (&items.impls[imp], imp)) {
            Some((block, _)) => {
                let name = match block.target {
                    Some(flux_hir::ImplTarget::Struct(s)) => items.structs[s].name.to_string(),
                    Some(flux_hir::ImplTarget::Primitive(p)) => p.to_string(),
                    None => String::new(),
                };
                let params: Vec<String> = block
                    .generics
                    .params
                    .iter()
                    .map(|p| p.name.to_string())
                    .collect();
                (name, params)
            }
            // A free function, by its module, which rustc names from
            // `std`, as the program links it: `std::cmp::max`.
            None => {
                let module = items.module_path(data.loc.module);
                let module = module.strip_prefix("core").unwrap_or(&module);
                (format!("std{module}"), Vec::new())
            }
        };
        let turbofish = |list: &[String]| {
            if list.is_empty() {
                String::new()
            } else {
                format!("::<{}>", list.join(", "))
            }
        };
        let shown: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let (owner_args, own_args) = shown.split_at(params.len().min(shown.len()));
        let path = format!(
            "{name}{}::{}{}",
            turbofish(owner_args),
            data.name,
            turbofish(own_args)
        );
        let generic_path = format!("{name}{}::{}", turbofish(&params), data.name);
        (path, generic_path)
    }

    /// A call of trait `t`'s `function` with generic arguments `args`
    /// (`Self` first) in a constant context: as rustc's `core` makes its
    /// traits' impls for its own types conditionally const, unstably,
    /// E0658 for those, and E0015 for other impls.
    fn const_trait_call(
        &mut self,
        t: flux_hir::TraitId,
        function: FunctionId,
        args: &[Ty],
        (call, whole): (TextRange, TextRange),
        context: &str,
    ) {
        let items = self.items;
        let data = &items.functions[function];
        let tr = &items.traits[t];
        // `<i32 as PartialEq>::eq`: without trailing arguments that are
        // `Self` by default.
        let params = &tr.generics.params;
        let self_ty = args.first().copied().unwrap_or(Ty::ERROR);
        let mut shown = &args[1.min(args.len())..(1 + params.len()).min(args.len())];
        while let [rest @ .., last] = shown
            && *last == self_ty
            && params[rest.len()].default.is_some()
        {
            shown = rest;
        }
        let trait_args = if shown.is_empty() {
            String::new()
        } else {
            let list: Vec<String> = shown.iter().map(|a| a.to_string()).collect();
            format!("<{}>", list.join(", "))
        };
        let path = format!("<{self_ty} as {}{trait_args}>::{}", tr.name, data.name);
        let what = if data.has_self {
            "method"
        } else {
            "associated function"
        };
        let core = items.is_core(tr.loc.module);
        if core && args.iter().all(|&a| self.const_operand(a)) {
            self.error(
                call,
                format!("cannot call conditionally-const {what} `{path}` in {context}"),
            );
            self.error(
                whole,
                format!("`{}` is not yet stable as a const trait", tr.name),
            );
        } else {
            self.error(
                call,
                format!("cannot call non-const {what} `{path}` in {context}"),
            );
        }
    }

    fn run_deferred(&mut self, reported: &mut HashSet<InferVar>) {
        // Operands of `-`, where a literal may reach one past the maximum,
        // with the negation, where rustc reports one out of range.
        let negated: HashMap<ExprId, ExprId> = self
            .body
            .exprs
            .iter()
            .filter_map(|(id, e)| match e {
                Expr::Unary {
                    op: UnaryOp::Neg,
                    operand,
                } => Some((*operand, id)),
                _ => None,
            })
            .collect();
        for deferred in std::mem::take(&mut self.deferred) {
            match deferred {
                Deferred::ConstUnstable {
                    function,
                    args,
                    call,
                    whole,
                    context,
                } => {
                    let args: Vec<Ty> = args.iter().map(|&a| self.resolve(a, reported)).collect();
                    let data = &self.items.functions[function];
                    if let Some(flux_hir::Owner::Trait(t)) = data.owner {
                        self.const_trait_call(t, function, &args, (call, whole), context);
                        continue;
                    }
                    let what = match data.owner {
                        Some(_) if data.has_self => "method",
                        Some(_) => "associated function",
                        None => "function",
                    };
                    let (path, generic_path) = self.unstable_paths(function, &args);
                    self.error(
                        call,
                        format!("cannot call conditionally-const {what} `{path}` in {context}"),
                    );
                    self.error(
                        whole,
                        format!("`{generic_path}` is not yet stable as a const fn"),
                    );
                }
                Deferred::ConstPat { expr, in_range } => {
                    let range = self.range(expr);
                    let args = self.consts.get(expr).map(|(_, args)| args.clone());
                    let generic = args.into_iter().flatten().any(|a| {
                        let a = self.resolve(a, reported);
                        a.has_params()
                    });
                    if generic {
                        self.error(
                            range,
                            "constant pattern cannot depend on generic parameters",
                        );
                        continue;
                    }
                    let ty = self.expr_types.get(expr).copied().unwrap_or(Ty::ERROR);
                    let ty = self.resolve(ty, reported);
                    // A range reports its other types itself.
                    if !in_range
                        && !matches!(
                            ty.kind(),
                            TyKind::Int(_) | TyKind::Bool | TyKind::Char | TyKind::Error
                        )
                    {
                        self.error(
                            range,
                            format!("constants of type `{ty}` in patterns are not supported yet"),
                        );
                    }
                }
                Deferred::RangePat { pat, ty } => {
                    let ty = self.resolve(ty, reported);
                    if !matches!(ty.kind(), TyKind::Int(_) | TyKind::Char | TyKind::Error) {
                        let range = self.source_map.pats.get(pat).copied().unwrap_or_default();
                        self.error(
                            range,
                            "only `char` and numeric types are allowed in range patterns",
                        );
                    }
                }
                Deferred::IntLiteral { expr, value } => {
                    let Some(int) = self.expr_types.get(expr).and_then(|ty| ty.as_int()) else {
                        continue;
                    };
                    let max = int.max();
                    let negation = negated.get(&expr).copied();
                    let limit = if negation.is_some() && int.is_signed() {
                        max + 1
                    } else {
                        max
                    };
                    if value > limit {
                        self.error(
                            self.range(negation.unwrap_or(expr)),
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
                    if !ok && self.negated_pats.contains(&expr) {
                        self.error(
                            self.range(expr),
                            format!("the trait bound `{ty}: Neg` is not satisfied"),
                        );
                    } else if !ok {
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
                        (
                            BinaryOp::Cmp(_),
                            TyKind::Int(_) | TyKind::Bool | TyKind::Char | TyKind::Tuple([]),
                        ) => true,
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
                        self.error(range, binary_error(op, &ty, &rhs));
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
                        (TyKind::Int(_) | TyKind::Bool | TyKind::Char, TyKind::Int(_)) => true,
                        (TyKind::Int(IntTy::U8), TyKind::Char) => true,
                        // A reference as a pointer to what it points to,
                        // or to an array's first element.
                        (TyKind::Ref(m, pointee), TyKind::RawPtr(to_m, target)) => {
                            (m == to_m || m == Mutability::Mut)
                                && (pointee == target
                                    || pointee.sequence_elem() == Some(target)
                                        && matches!(pointee.kind(), TyKind::Array(..)))
                        }
                        // A pointer to a sized type takes no length; a
                        // pointer to a slice or `str` keeps its length.
                        (TyKind::RawPtr(_, a), TyKind::RawPtr(_, b)) => {
                            !b.is_unsized() || a.is_unsized()
                        }
                        (TyKind::RawPtr(_, pointee), TyKind::Int(_)) => !pointee.is_unsized(),
                        (TyKind::Int(_), TyKind::RawPtr(_, pointee)) => !pointee.is_unsized(),
                        // A fieldless enum, to its discriminant.
                        (TyKind::Adt(adt, _), TyKind::Int(_)) => {
                            self.sigs.variants(from).is_some_and(|variants| {
                                variants.iter().all(|fields| fields.is_empty())
                            }) && crate::traits::drop_impl(self.items, self.sigs, adt.strukt)
                                .is_none()
                        }
                        _ => from == to,
                    };
                    if !ok {
                        let message = if from.fn_sig().is_some() || to.fn_sig().is_some() {
                            "casts of function items and pointers are not supported yet".to_owned()
                        } else if let (TyKind::RawPtr(..), TyKind::RawPtr(_, b)) =
                            (from.kind(), to.kind())
                            && b.is_unsized()
                        {
                            format!("cannot cast thin pointer `{from}` to wide pointer `{to}`")
                        } else if to == Ty::BOOL && from.is_pointer() {
                            format!("cannot cast `{from}` as `bool`")
                        } else if from.is_pointer() || to.is_pointer() {
                            format!("casting `{from}` as `{to}` is invalid")
                        } else if to == Ty::BOOL && (from.as_int().is_some() || from == Ty::CHAR) {
                            format!("cannot cast `{from}` as `bool`")
                        } else if to == Ty::CHAR && (from.as_int().is_some() || from == Ty::BOOL) {
                            format!("only `u8` can be cast as `char`, not `{from}`")
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
                    // A reference to a sized value is a pointer, as a raw
                    // pointer is.
                    let ok = match ty.kind() {
                        TyKind::Ref(_, pointee) => !matches!(
                            pointee.kind(),
                            TyKind::Slice(_) | TyKind::Str | TyKind::CStr
                        ),
                        TyKind::Int(_)
                        | TyKind::Char
                        | TyKind::RawPtr(..)
                        | TyKind::Error
                        | TyKind::Never => true,
                        _ => false,
                    };
                    if let Some(cast_to) = cast_to {
                        self.error(
                            self.range(expr),
                            format!(
                                "can't pass `{ty}` to variadic function; cast it to `{cast_to}`"
                            ),
                        );
                    } else if let TyKind::FnDef(_) = ty.kind() {
                        self.error(
                            self.range(expr),
                            "can't pass a function item to a variadic function",
                        );
                    } else if !ok {
                        // rustc passes anything else as it is.
                        self.error(
                            self.range(expr),
                            format!("passing `{ty}` to a variadic function is not supported yet"),
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
        BinaryOp::Cmp(CmpOp::Lt) => ("partial_ord", "lt"),
        BinaryOp::Cmp(CmpOp::Le) => ("partial_ord", "le"),
        BinaryOp::Cmp(CmpOp::Gt) => ("partial_ord", "gt"),
        BinaryOp::Cmp(CmpOp::Ge) => ("partial_ord", "ge"),
        BinaryOp::Logic(_) | BinaryOp::Assign(None) => return None,
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
