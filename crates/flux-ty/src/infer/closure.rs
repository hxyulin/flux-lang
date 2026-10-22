//! Closures in a body: their types, calls of values through the
//! `Fn`-family traits, and what each closure captures.
//!
//! A closure's type is unique to its expression ([`TyKind::Closure`]),
//! with its owner's generic parameters, then its kind, signature and the
//! types of what it captures, each inferred like any type. Its
//! signature comes from what it is passed to where that is a call whose
//! bounds name a `Fn`-family trait for the parameter (rustc's
//! `deduce_closure_signature`), which also fixes its kind; else from its
//! annotations and its body.
//!
//! Once the body is inferred, each closure's captures are found as
//! rustc's edition-2021 analysis finds them, innermost first: each use
//! of a place outside the closure (a binding, its fields and what its
//! references point to) is a copy, a move, or a shared or mutable
//! borrow; moved places are captured by value, others by reference
//! (all by value with `move`), a place used whole and in part is
//! captured whole, and places are cut at the first dereference when
//! captured by value and at the last one of a shared reference when
//! borrowed. A closure that moves what it captures is `FnOnce`, one
//! that changes it `FnMut`, any other `Fn`.

use std::collections::HashSet;

use flux_hir::{
    BinaryOp, BindingId, Expr, ExprId, FunctionId, Pat, PatId, Res, Stmt, StructId, TypeRefId,
};
use flux_intern::Symbol;
use flux_syntax::TextRange;

use super::{Ctx, Method};
use crate::traits::{Predicate, TraitRef};
use crate::ty::{ClosureArgs, ClosureId, ClosureKind, Mutability, Ty, TyKind};

/// How a closure holds a place it captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    ByRef(Mutability),
    ByValue,
}

impl CaptureMode {
    /// Shared borrows, then mutable ones, then by value.
    fn rank(self) -> u8 {
        match self {
            CaptureMode::ByRef(Mutability::Not) => 0,
            CaptureMode::ByRef(Mutability::Mut) => 1,
            CaptureMode::ByValue => 2,
        }
    }
}

/// A step from a binding to a captured place in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureProj {
    Deref,
    Field(u32),
}

/// A place a closure captures: a binding outside it and a path into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub binding: BindingId,
    pub projection: Box<[CaptureProj]>,
    pub mode: CaptureMode,
    /// The place's type.
    pub ty: Ty,
    /// The use that decided how it is captured, where rustc reports
    /// problems with the capture, and whether that use assigns it.
    pub range: TextRange,
    pub assigned: bool,
}

impl Capture {
    /// The type of the closure's field holding it.
    pub fn upvar_ty(&self) -> Ty {
        match self.mode {
            CaptureMode::ByValue => self.ty,
            CaptureMode::ByRef(m) => Ty::new(TyKind::Ref(m, self.ty)),
        }
    }
}

/// What type checking found about a closure expression.
#[derive(Debug, Clone)]
pub struct ClosureInfo {
    pub ty: Ty,
    pub kind: ClosureKind,
    /// In the order of the closure's fields.
    pub captures: Box<[Capture]>,
}

/// The signature a closure is expected to have: from the bounds of the
/// parameter of the call it is passed to.
#[derive(Debug, Clone, Copy)]
pub(super) struct Hint {
    kind: ClosureKind,
    params: Ty,
    ret: Option<Ty>,
    /// The callee, where rustc reports a closure that does not fit.
    at: TextRange,
}

/// A closure whose captures are not known yet.
#[derive(Debug, Clone)]
pub(super) struct PendingClosure {
    expr: ExprId,
    ty: Ty,
    is_move: bool,
    /// The bindings declared in it, which it does not capture.
    locals: HashSet<BindingId>,
}

/// A call of a value of a closure's type, whose method (`call`,
/// `call_mut` or `call_once`) follows from the closure's kind.
#[derive(Debug, Clone, Copy)]
pub(super) struct ClosureCall {
    call: ExprId,
    closure: Ty,
    derefs: u32,
}

/// How a place is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UseKind {
    Copy,
    Move,
    Borrow(Mutability),
    /// Assigned, which captures as a mutable borrow does.
    Assign,
}

/// A place outside the closure being analyzed, used in it.
type Place = (BindingId, Vec<CaptureProj>);

/// The uses of places outside one closure, and its own bindings.
struct Uses<'l> {
    locals: &'l HashSet<BindingId>,
    found: Vec<(Place, UseKind, TextRange)>,
}

impl Uses<'_> {
    fn add(&mut self, place: Place, kind: UseKind, range: TextRange) {
        if !self.locals.contains(&place.0) {
            self.found.push((place, kind, range));
        }
    }
}

impl Ctx<'_> {
    /// `|params| body`: its type, with the signature `hint` gives, its
    /// annotations, or new variables.
    pub(super) fn infer_closure(
        &mut self,
        id: ExprId,
        params: &[PatId],
        param_types: &[Option<TypeRefId>],
        ret: Option<TypeRefId>,
        body: ExprId,
        is_move: bool,
    ) -> Ty {
        let range = self.range(id);
        let Some(owner) = self.owner_fn.filter(|_| self.const_context.is_none()) else {
            let context = self.const_context.unwrap_or("constants");
            self.error(
                range,
                format!("closures in {context} are not supported yet"),
            );
            return Ty::ERROR;
        };
        let mut hint = self.closure_hints.remove(id);
        // Where a function pointer is expected, its signature.
        if hint.is_none()
            && let Some(&expected) = self.expected.get(id)
            && let TyKind::FnPtr(sig) = self.shallow(expected).kind()
        {
            hint = Some(Hint {
                kind: ClosureKind::Fn,
                params: Ty::tuple(sig.params()),
                ret: Some(sig.ret()),
                at: range,
            });
        }
        // Where an opaque type's hidden type is expected (a closure
        // returned as `impl Fn(i32) -> i32`), its bounds'.
        if hint.is_none()
            && let Some(&expected) = self.expected.get(id)
        {
            hint = self.opaque_hint(expected, range);
        }
        // Where rustc reports a signature that does not fit: at the
        // callee it is passed to.
        let head = match hint {
            Some(hint) => hint.at,
            None => self.closure_head(id, params, param_types),
        };
        let mut hinted: Option<&'static [Ty]> =
            hint.and_then(|h| match self.shallow(h.params).kind() {
                TyKind::Tuple(list) => Some(list),
                _ => None,
            });
        if let Some(expected) = hinted
            && expected.len() != params.len()
        {
            let (want, got) = (expected.len(), params.len());
            self.error(
                head,
                format!(
                    "closure is expected to take {want} argument{}, but it takes {got} argument{}",
                    super::plural(want),
                    super::plural(got)
                ),
            );
            self.mismatched_closures.insert(id);
            hinted = None;
        }
        let mut types = Vec::with_capacity(params.len());
        for (i, &pat) in params.iter().enumerate() {
            let declared = param_types
                .get(i)
                .copied()
                .flatten()
                .map(|t| self.resolve_type_ref(t));
            let expected = hinted.and_then(|h| h.get(i)).copied();
            let ty = match (declared, expected) {
                (Some(declared), Some(expected)) => {
                    if !self.can_unify(declared, expected) {
                        if self.mismatched_closures.insert(id) {
                            self.error(head, "type mismatch in closure arguments");
                        }
                    } else {
                        self.unify(declared, expected);
                    }
                    declared
                }
                (Some(ty), None) | (None, Some(ty)) => ty,
                (None, None) => {
                    let at = self.source_map.pats.get(pat).copied().unwrap_or(range);
                    self.new_var(false, at)
                }
            };
            types.push(ty);
        }
        let declared_ret = ret.map(|t| self.resolve_type_ref(t));
        let ret_ty = match (declared_ret, hint.and_then(|h| h.ret)) {
            (Some(declared), Some(expected)) => {
                if self.can_unify(declared, expected) {
                    self.unify(declared, expected);
                }
                declared
            }
            (Some(ty), None) | (None, Some(ty)) => ty,
            (None, None) => self.new_var(false, range),
        };
        let kind = match hint {
            Some(hint) if hinted.is_some() || params.is_empty() => hint.kind.as_ty(),
            _ => self.new_var(false, range),
        };
        let upvars = self.new_var(false, range);
        let sig = Ty::tuple(&[Ty::tuple(&types), ret_ty]);
        let mut args: Vec<Ty> = (self.generics.iter().enumerate())
            .map(|(i, &name)| Ty::param(i as u32, name))
            .collect();
        args.extend([kind, sig, upvars]);
        let ty = Ty::closure(ClosureId { owner, expr: id }, &args);
        for (&pat, &ty) in params.iter().zip(&types) {
            self.check_pat(pat, ty, false);
        }
        let outer_ret = std::mem::replace(&mut self.ret, ret_ty);
        let mut locals = HashSet::new();
        for &pat in params {
            pat_bindings_into(self.body, pat, &mut locals);
        }
        expr_bindings(self.body, body, &mut locals);
        self.closure_locals.push(locals);
        self.expect(body, ret_ty);
        let body_ty = self.infer_expr(body);
        self.coerce(body_ty, ret_ty, body);
        let locals = self.closure_locals.pop().unwrap_or_default();
        self.ret = outer_ret;
        self.closures.push(PendingClosure {
            expr: id,
            ty,
            is_move,
            locals,
        });
        ty
    }

    /// `|a, b|` of a closure, where rustc reports problems with its
    /// signature: up to the last parameter's end and the `|` after it.
    fn closure_head(
        &self,
        id: ExprId,
        params: &[PatId],
        param_types: &[Option<TypeRefId>],
    ) -> TextRange {
        let range = self.range(id);
        let last = params
            .iter()
            .zip(param_types)
            .next_back()
            .map(|(&pat, ty)| {
                let ty_end = ty.and_then(|t| self.source_map.types.get(t).map(|r| r.end()));
                let pat_end = self.source_map.pats.get(pat).map(|r| r.end());
                ty_end.or(pat_end).unwrap_or(range.start())
            });
        let end = match last {
            Some(end) => end + 1,
            None => range.start() + 2,
        };
        TextRange::new(range.start(), end.min(range.end()))
    }

    /// The signature expected of a closure passed as a parameter of type
    /// `param` to `function` with generic arguments `args`: from its
    /// `Fn`-family bounds on that parameter, the most restrictive one's
    /// kind, and their `Output`.
    pub(super) fn closure_hint(
        &mut self,
        function: FunctionId,
        args: &[Ty],
        param: Ty,
    ) -> Option<Hint> {
        let param = self.shallow(param);
        if !matches!(param.kind(), TyKind::Infer(_) | TyKind::Param(_)) {
            return None;
        }
        let (items, sigs) = (self.items, self.sigs);
        let mut kind: Option<ClosureKind> = None;
        let mut params = None;
        for p in crate::traits::predicates_of(items, sigs, function) {
            let Some(k) = crate::fn_trait_kind(items, p.trait_ref.trait_) else {
                continue;
            };
            let p = p.subst(args);
            if self.shallow(p.self_ty) != param {
                continue;
            }
            kind = Some(kind.map_or(k, |old| old.min(k)));
            params = p.trait_ref.args.first().copied();
        }
        let (kind, params) = (kind?, params?);
        let range = TextRange::default();
        let params = self.normalize(params, range);
        let ret = crate::traits::declared_projections(items, sigs, function)
            .into_iter()
            .find_map(|p| {
                let is_output = crate::fn_trait_kind(items, p.alias.trait_)
                    == Some(ClosureKind::FnOnce)
                    && p.alias.name.as_str() == "Output";
                (is_output && self.shallow(p.alias.args[0].subst(args)) == param)
                    .then(|| p.ty.subst(args))
            })
            .map(|ret| self.normalize(ret, range));
        Some(Hint {
            kind,
            params,
            ret,
            at: TextRange::default(),
        })
    }

    /// The signature expected of a closure whose type is `expected`, the
    /// hidden type of one of the body's opaque types, from that type's
    /// `Fn`-family bounds, as rustc deduces it from them.
    fn opaque_hint(&mut self, expected: Ty, at: TextRange) -> Option<Hint> {
        let TyKind::Infer(var) = self.shallow(expected).kind() else {
            return None;
        };
        let owner = self.owner_fn?;
        let vars = self.own_opaques.clone();
        let index = vars
            .iter()
            .position(|&v| matches!(self.shallow(v).kind(), TyKind::Infer(w) if w == var))?;
        let sig = self.sigs.opaques.get(owner)?.get(index)?;
        let mut args = self.own_opaque_args.to_vec();
        args.push(expected);
        let items = self.items;
        let mut kind: Option<ClosureKind> = None;
        let mut params = None;
        for p in sig.predicates.iter() {
            let Some(k) = crate::fn_trait_kind(items, p.trait_ref.trait_) else {
                continue;
            };
            kind = Some(kind.map_or(k, |old| old.min(k)));
            params = p.trait_ref.args.first().map(|&a| a.subst(&args));
        }
        let ret = sig.projections.iter().find_map(|p| {
            let is_output = crate::fn_trait_kind(items, p.alias.trait_)
                == Some(ClosureKind::FnOnce)
                && p.alias.name.as_str() == "Output";
            is_output.then(|| p.ty.subst(&args))
        });
        let ret = ret.map(|r| self.normalize(r, at));
        Some(Hint {
            kind: kind?,
            params: params?,
            ret,
            at,
        })
    }

    /// Notes the signatures expected of the closures among `args`, passed
    /// to `function` with generic arguments `generic_args` as parameters
    /// of types `params`.
    pub(super) fn hint_closure_args(
        &mut self,
        function: FunctionId,
        generic_args: &[Ty],
        params: &[Ty],
        args: &[ExprId],
        at: TextRange,
    ) {
        for (&arg, &param) in args.iter().zip(params) {
            if !matches!(self.body.exprs[arg], Expr::Closure { .. }) {
                continue;
            }
            if let Some(hint) = self.closure_hint(function, generic_args, param) {
                self.closure_hints.insert(arg, Hint { at, ..hint });
            }
        }
    }

    /// `Fn::call` and the other methods may be called by name only
    /// with the unstable `fn_traits` feature.
    pub(super) fn check_fn_trait_method(&mut self, function: FunctionId, at: TextRange) {
        let items = self.items;
        let Some(flux_hir::Owner::Trait(t)) = items.functions[function].owner else {
            return;
        };
        if crate::fn_trait_kind(items, t).is_some() && !items.is_core(self.module) {
            self.error(at, "use of unstable library feature `fn_traits`");
        }
    }

    /// Whether `expr` is a closure, which arguments infer after the
    /// others, as rustc does, so that it may know their types.
    pub(super) fn is_closure(&self, expr: ExprId) -> bool {
        matches!(self.body.exprs[expr], Expr::Closure { .. })
    }

    /// A call of a value, `f(x)`: of a closure, or of a type parameter
    /// with a `Fn`-family bound, through references.
    pub(super) fn infer_value_call(
        &mut self,
        id: ExprId,
        callee: ExprId,
        callee_ty: Ty,
        args: &[ExprId],
    ) -> Ty {
        let (base, derefs) = self.autoderef(callee_ty);
        match base.kind() {
            TyKind::Closure(_, closure_args) => {
                let parts = ClosureArgs::split(closure_args);
                let params = match self.shallow(parts.params()).kind() {
                    TyKind::Tuple(list) => list,
                    _ => &[],
                };
                self.check_value_call_args(callee, params, args);
                self.closure_calls.push(ClosureCall {
                    call: id,
                    closure: base,
                    derefs,
                });
                parts.ret()
            }
            TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) => {
                let items = self.items;
                // An opaque type's bounds are its own.
                let base = self.known(base);
                let own = match base.kind() {
                    TyKind::Opaque(opaque) => crate::traits::opaque_bounds(self.sigs, opaque),
                    _ => Vec::new(),
                };
                let found = [ClosureKind::Fn, ClosureKind::FnMut, ClosureKind::FnOnce]
                    .into_iter()
                    .find_map(|kind| {
                        let t = crate::fn_trait(items, kind)?;
                        let bound = self
                            .param_env
                            .iter()
                            .chain(&own)
                            .find(|p| p.trait_ref.trait_ == t && p.self_ty == base)?;
                        Some((kind, bound.trait_ref))
                    });
                let Some((kind, trait_ref)) = found else {
                    self.not_callable(callee, callee_ty);
                    self.infer_args(args);
                    return Ty::ERROR;
                };
                let tupled = trait_ref.args.first().copied().unwrap_or(Ty::ERROR);
                let params = match tupled.kind() {
                    TyKind::Tuple(list) => list,
                    _ => &[],
                };
                self.check_value_call_args(callee, params, args);
                self.record_fn_call(id, kind, base, tupled, derefs);
                let Some(once) = crate::fn_trait(items, ClosureKind::FnOnce) else {
                    return Ty::ERROR;
                };
                let once = TraitRef {
                    trait_: once,
                    name: items.traits[once].name,
                    args: Ty::list(&[tupled]),
                };
                let output = crate::traits::alias_of(once, base, Symbol::intern("Output"));
                self.normalize(Ty::alias(output), self.range(id))
            }
            TyKind::Infer(var) if !self.unknown(var).0 => {
                self.annotations_needed(callee, var, self.range(callee));
                self.infer_args(args);
                Ty::ERROR
            }
            _ => {
                self.not_callable(callee, callee_ty);
                self.infer_args(args);
                Ty::ERROR
            }
        }
    }

    /// The arguments of a call of a value whose parameters are `params`.
    fn check_value_call_args(&mut self, callee: ExprId, params: &[Ty], args: &[ExprId]) {
        if params.len() != args.len() {
            let (want, got) = (params.len(), args.len());
            let verb = if got == 1 { "was" } else { "were" };
            self.error(
                self.range(callee),
                format!(
                    "this function takes {want} argument{} but {got} argument{} {verb} supplied",
                    super::plural(want),
                    super::plural(got)
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
    }

    /// Records call `id` as a call of the `kind` trait's method on a
    /// `self_ty`, with the arguments as the tuple `tupled`.
    fn record_fn_call(
        &mut self,
        id: ExprId,
        kind: ClosureKind,
        self_ty: Ty,
        tupled: Ty,
        derefs: u32,
    ) {
        let items = self.items;
        let Some(t) = crate::fn_trait(items, kind) else {
            return;
        };
        let (name, autoref) = match kind {
            ClosureKind::Fn => ("call", Some(Mutability::Not)),
            ClosureKind::FnMut => ("call_mut", Some(Mutability::Mut)),
            ClosureKind::FnOnce => ("call_once", None),
        };
        let Some(function) = items.traits[t].function(items, Symbol::intern(name)) else {
            return;
        };
        self.methods.insert(
            id,
            Method {
                function,
                derefs,
                autoref,
                unsize: false,
            },
        );
        self.calls.insert(id, (function, vec![self_ty, tupled]));
    }

    /// For the solver: whether a `Fn`-family bound of kind `kind` with
    /// parameters `params` on closure type `closure` holds, recording the
    /// kind to check if the closure's is not known yet.
    pub(super) fn select_closure_fn(&mut self, closure: Ty, kind: ClosureKind, params: Ty) -> bool {
        let TyKind::Closure(id, args) = closure.kind() else {
            return false;
        };
        let parts = ClosureArgs::split(args);
        if !self.unify(params, parts.params()) {
            // Reported at the closure already.
            return self.mismatched_closures.contains(&id.expr);
        }
        match ClosureKind::from_ty(self.shallow(parts.kind)) {
            Some(actual) if actual > kind => {
                self.report_closure_kind(id.expr, kind, actual);
                true
            }
            Some(_) => true,
            None => {
                self.kind_requirements.push((id.expr, kind));
                true
            }
        }
    }

    fn report_closure_kind(&mut self, expr: ExprId, expected: ClosureKind, actual: ClosureKind) {
        if self.mismatched_closures.insert(expr) {
            self.error(
                self.range(expr),
                format!(
                    "expected a closure that implements the `{}` trait, but this closure only implements `{}`",
                    expected.name(),
                    actual.name()
                ),
            );
        }
    }

    /// Finds what each closure captures and its kind, innermost first,
    /// then the methods its calls call and whether the bounds it was
    /// expected to meet hold.
    pub(super) fn analyze_closures(&mut self) {
        let mut pending = std::mem::take(&mut self.closures);
        pending.sort_by_key(|c| c.expr.into_raw());
        let mut calls: rustc_hash::FxHashMap<ExprId, Vec<ClosureCall>> = Default::default();
        for call in std::mem::take(&mut self.closure_calls) {
            if let TyKind::Closure(id, _) = call.closure.kind() {
                calls.entry(id.expr).or_default().push(call);
            }
        }
        let mut required: rustc_hash::FxHashMap<ExprId, Vec<ClosureKind>> = Default::default();
        for (expr, kind) in std::mem::take(&mut self.kind_requirements) {
            required.entry(expr).or_default().push(kind);
        }
        let reified = std::mem::take(&mut self.reified_closures);
        for closure in pending {
            let info = self.analyze_closure(&closure);
            for &(expr, closure_ty, pointer) in &reified {
                if self.shallow_closure_expr(closure_ty) == Some(closure.expr)
                    && !info.captures.is_empty()
                {
                    let range = self.range(self.blame(expr));
                    self.mismatch(pointer, closure_ty, range);
                }
            }
            let TyKind::Closure(_, args) = closure.ty.kind() else {
                continue;
            };
            let parts = ClosureArgs::split(args);
            let upvars: Vec<Ty> = info.captures.iter().map(Capture::upvar_ty).collect();
            self.unify(parts.upvars, Ty::tuple(&upvars));
            let kind = match ClosureKind::from_ty(self.shallow(parts.kind)) {
                Some(kind) => kind,
                None => {
                    self.unify(parts.kind, info.kind.as_ty());
                    info.kind
                }
            };
            self.closure_infos
                .insert(closure.expr, ClosureInfo { kind, ..info });
            // Calls of it, now that its kind is known.
            for call in calls.remove(&closure.expr).unwrap_or_default() {
                let params = parts.params();
                self.record_fn_call(call.call, kind, call.closure, params, call.derefs);
            }
            for expected in required.remove(&closure.expr).unwrap_or_default() {
                if kind > expected {
                    self.report_closure_kind(closure.expr, expected, kind);
                }
            }
        }
    }

    fn shallow_closure_expr(&mut self, ty: Ty) -> Option<ExprId> {
        match self.shallow(ty).kind() {
            TyKind::Closure(id, _) => Some(id.expr),
            _ => None,
        }
    }

    /// Integer variables take their default, `i32`, as rustc's fallback
    /// does before it analyzes closures.
    pub(super) fn default_int_vars(&mut self) {
        for (value, _) in &mut self.vars {
            if let super::VarValue::Unknown { int: true, .. } = value {
                *value = super::VarValue::Known(Ty::int(crate::IntTy::I32));
            }
        }
        self.select_obligations();
    }

    /// The captures and kind of one closure, whose nested closures are
    /// analyzed already.
    fn analyze_closure(&mut self, closure: &PendingClosure) -> ClosureInfo {
        let body = self.body;
        let Expr::Closure {
            body: closure_body, ..
        } = &body.exprs[closure.expr]
        else {
            unreachable!("pending closures are closures")
        };
        let mut uses = Uses {
            locals: &closure.locals,
            found: Vec::new(),
        };
        self.walk(*closure_body, &mut uses);
        let found = uses.found;
        // The kind, from how the places are used.
        let mut kind = ClosureKind::Fn;
        for (_, use_kind, _) in &found {
            let k = match use_kind {
                UseKind::Move => ClosureKind::FnOnce,
                UseKind::Borrow(Mutability::Mut) | UseKind::Assign => ClosureKind::FnMut,
                _ => ClosureKind::Fn,
            };
            kind = kind.max(k);
        }
        // The places, as few and as short as rustc's.
        let mut captures: Vec<(Place, CaptureMode, TextRange, bool)> = Vec::new();
        for (place, use_kind, range) in found {
            let mode = match (use_kind, closure.is_move) {
                (UseKind::Move, _) | (_, true) => CaptureMode::ByValue,
                (UseKind::Copy, false) => CaptureMode::ByRef(Mutability::Not),
                (UseKind::Borrow(m), false) => CaptureMode::ByRef(m),
                (UseKind::Assign, false) => CaptureMode::ByRef(Mutability::Mut),
            };
            let place = self.truncate_capture(place, mode);
            let assigned = use_kind == UseKind::Assign;
            insert_capture(&mut captures, (place, mode, range, assigned));
        }
        let captures = captures
            .into_iter()
            .map(|((binding, projection), mode, range, assigned)| {
                let ty = self.capture_place_ty(binding, &projection);
                Capture {
                    binding,
                    projection: projection.into(),
                    mode,
                    ty,
                    range,
                    assigned,
                }
            })
            .collect();
        ClosureInfo {
            ty: closure.ty,
            kind,
            captures,
        }
    }

    /// `place` as rustc captures it in `mode`: before a dereference of a
    /// raw pointer; by value, before the first dereference; by shared
    /// reference, after the last dereference of a shared reference.
    fn truncate_capture(&mut self, (binding, mut proj): Place, mode: CaptureMode) -> Place {
        let mut ty = self.known_binding_ty(binding);
        let mut first_deref = None;
        let mut last_shared = None;
        for (i, &p) in proj.iter().enumerate() {
            match p {
                CaptureProj::Deref => {
                    match ty.kind() {
                        TyKind::RawPtr(..) => {
                            proj.truncate(i);
                            return (binding, proj);
                        }
                        TyKind::Ref(Mutability::Not, _) => last_shared = Some(i),
                        _ => {}
                    }
                    first_deref.get_or_insert(i);
                }
                CaptureProj::Field(_) => {}
            }
            ty = self.project_ty(ty, p);
        }
        match (mode, first_deref, last_shared) {
            (CaptureMode::ByValue, Some(i), _) => proj.truncate(i),
            (CaptureMode::ByRef(Mutability::Not), _, Some(i)) => proj.truncate(i + 1),
            _ => {}
        }
        (binding, proj)
    }

    fn known_binding_ty(&mut self, binding: BindingId) -> Ty {
        let ty = self
            .binding_types
            .get(binding)
            .copied()
            .unwrap_or(Ty::ERROR);
        self.known(ty)
    }

    /// The type of a part of a value of type `ty`.
    fn project_ty(&mut self, ty: Ty, proj: CaptureProj) -> Ty {
        let ty = self.known(ty);
        match (proj, ty.kind()) {
            (CaptureProj::Deref, TyKind::Ref(_, t) | TyKind::RawPtr(_, t)) => self.known(t),
            (CaptureProj::Field(i), _) => {
                let field = self
                    .sigs
                    .fields(ty)
                    .and_then(|f| f.get(i as usize).copied())
                    .unwrap_or(Ty::ERROR);
                let field = self.normalize(field, TextRange::default());
                self.known(field)
            }
            _ => Ty::ERROR,
        }
    }

    fn capture_place_ty(&mut self, binding: BindingId, proj: &[CaptureProj]) -> Ty {
        let mut ty = self.known_binding_ty(binding);
        for &p in proj {
            ty = self.project_ty(ty, p);
        }
        ty
    }

    /// The place `expr` denotes, if it is a place: a binding, or a field
    /// or dereference of a place.
    /// An element of an array or slice is captured as the whole array
    /// or slice, as in rustc.
    fn capture_place(&self, expr: ExprId) -> Option<Place> {
        self.capture_place_in(expr).map(|(place, _)| place)
    }

    /// [`Self::capture_place`], and whether it went through an index, after
    /// which the place stops.
    fn capture_place_in(&self, expr: ExprId) -> Option<(Place, bool)> {
        let derefs = |proj: &mut Vec<CaptureProj>| {
            let n = self.field_derefs.get(expr).copied().unwrap_or(0);
            proj.extend((0..n).map(|_| CaptureProj::Deref));
        };
        match self.body.exprs[expr] {
            Expr::Path(Res::Local(binding)) => Some(((binding, Vec::new()), false)),
            Expr::Field { expr: base, .. } => {
                let ((binding, mut proj), indexed) = self.capture_place_in(base)?;
                if !indexed {
                    derefs(&mut proj);
                    proj.push(CaptureProj::Field(self.field_indices.get(expr).copied()?));
                }
                Some(((binding, proj), indexed))
            }
            Expr::Index { base, .. } => {
                let ((binding, mut proj), indexed) = self.capture_place_in(base)?;
                if !indexed {
                    derefs(&mut proj);
                }
                Some(((binding, proj), true))
            }
            Expr::Unary {
                op: flux_hir::UnaryOp::Deref,
                operand,
            } => {
                let ((binding, mut proj), indexed) = self.capture_place_in(operand)?;
                if !indexed {
                    proj.push(CaptureProj::Deref);
                }
                Some(((binding, proj), indexed))
            }
            _ => None,
        }
    }

    /// The indices in the place expression `expr`, which are evaluated
    /// for their values.
    fn walk_indices(&mut self, expr: ExprId, uses: &mut Uses<'_>) {
        let mut e = expr;
        loop {
            match self.body.exprs[e] {
                Expr::Field { expr: base, .. }
                | Expr::Unary {
                    op: flux_hir::UnaryOp::Deref,
                    operand: base,
                } => e = base,
                Expr::Index { base, index } => {
                    self.consume(index, uses);
                    e = base;
                }
                _ => return,
            }
        }
    }

    fn is_copy_ty(&mut self, ty: Ty) -> bool {
        let ty = self.known(ty);
        crate::traits::is_copy(self.items, self.sigs, &self.param_env, ty)
    }

    /// `expr` used by value: a place is copied or moved.
    fn consume(&mut self, expr: ExprId, uses: &mut Uses<'_>) {
        self.walk_indices(expr, uses);
        if let Some(&m) = self.reborrows.get(expr)
            && let Some((binding, mut proj)) = self.capture_place(expr)
        {
            let derefs = self.deref_coercions.get(expr).copied().unwrap_or(1);
            proj.extend((0..derefs).map(|_| CaptureProj::Deref));
            uses.add((binding, proj), UseKind::Borrow(m), self.range(expr));
            return;
        }
        match self.capture_place(expr) {
            Some(place) => {
                let ty = self.expr_types.get(expr).copied().unwrap_or(Ty::ERROR);
                let kind = if self.is_copy_ty(ty) {
                    UseKind::Copy
                } else {
                    UseKind::Move
                };
                uses.add(place, kind, self.range(expr));
            }
            None => self.walk(expr, uses),
        }
    }

    /// `expr` borrowed, or written (`Borrow(Mut)`).
    fn borrow(&mut self, expr: ExprId, m: Option<Mutability>, uses: &mut Uses<'_>) {
        self.walk_indices(expr, uses);
        match self.capture_place(expr) {
            Some(place) => {
                let kind = match m {
                    Some(m) => UseKind::Borrow(m),
                    None => UseKind::Assign,
                };
                uses.add(place, kind, self.range(expr));
            }
            None => self.walk(expr, uses),
        }
    }

    /// The receiver of a method or of a call of a value, dereferenced
    /// `derefs` times, then borrowed by `autoref` or used by value.
    fn adjusted(
        &mut self,
        expr: ExprId,
        derefs: u32,
        autoref: Option<Mutability>,
        uses: &mut Uses<'_>,
    ) {
        self.walk_indices(expr, uses);
        let Some((binding, mut proj)) = self.capture_place(expr) else {
            self.walk(expr, uses);
            return;
        };
        proj.extend((0..derefs).map(|_| CaptureProj::Deref));
        let kind = match autoref {
            Some(m) => UseKind::Borrow(m),
            None => {
                let ty = self.capture_place_ty(binding, &proj);
                if self.is_copy_ty(ty) {
                    UseKind::Copy
                } else {
                    UseKind::Move
                }
            }
        };
        uses.add((binding, proj), kind, self.range(expr));
    }

    /// Whether `expr` is a `&mut` reference.
    fn is_mut_ref(&mut self, expr: ExprId) -> bool {
        let ty = self.expr_types.get(expr).copied().unwrap_or(Ty::ERROR);
        matches!(self.known(ty).kind(), TyKind::Ref(Mutability::Mut, _))
    }

    /// `expr`, a reference, passed on as a shared one: copied, or
    /// reborrowed (`&*r`) if it is a `&mut`.
    fn shared_ref(&mut self, expr: ExprId, uses: &mut Uses<'_>) {
        if self.is_mut_ref(expr) {
            self.adjusted(expr, 1, Some(Mutability::Not), uses);
        } else {
            self.consume(expr, uses);
        }
    }

    /// The uses of places in evaluating `expr` for its value.
    fn walk(&mut self, expr: ExprId, uses: &mut Uses<'_>) {
        let body = self.body;
        match &body.exprs[expr] {
            Expr::Missing | Expr::Literal(_) | Expr::Continue { .. } => {}
            Expr::Path(Res::Local(_)) | Expr::Field { .. }
                if self.capture_place(expr).is_some() =>
            {
                self.consume(expr, uses);
            }
            Expr::Path(_) => {}
            Expr::Field { expr: base, .. } => self.walk(*base, uses),
            Expr::Index { .. } if self.capture_place(expr).is_some() => self.consume(expr, uses),
            Expr::Index { base, index } => {
                self.walk(*base, uses);
                self.consume(*index, uses);
            }
            Expr::Array(elements) => {
                for &e in elements.iter() {
                    self.consume(e, uses);
                }
            }
            Expr::Repeat { value, .. } => self.consume(*value, uses),
            Expr::Unary {
                op: flux_hir::UnaryOp::Deref,
                operand,
            } => {
                if self.capture_place(expr).is_some() {
                    self.consume(expr, uses);
                } else {
                    self.walk(*operand, uses);
                }
            }
            Expr::Unary { operand, .. } | Expr::Cast { expr: operand, .. } => {
                self.consume(*operand, uses);
            }
            Expr::Try { expr: operand } => self.consume(*operand, uses),
            Expr::Tuple(fields) => {
                for &f in fields.iter() {
                    self.consume(f, uses);
                }
            }
            Expr::Record { fields, spread, .. } => {
                for &(_, f) in fields.iter() {
                    self.consume(f, uses);
                }
                if let Some(spread) = spread {
                    self.consume(*spread, uses);
                }
            }
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts.iter() {
                    match *stmt {
                        Stmt::Let {
                            pat,
                            init,
                            else_branch,
                            ..
                        } => {
                            if let Some(init) = init {
                                self.walk_let(pat, init, uses);
                            }
                            if let Some(else_branch) = else_branch {
                                self.walk(else_branch, uses);
                            }
                        }
                        Stmt::Expr { expr, .. } => self.consume(expr, uses),
                    }
                }
                if let Some(tail) = tail {
                    self.consume(*tail, uses);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.consume(*condition, uses);
                self.consume(*then_branch, uses);
                if let Some(e) = else_branch {
                    self.consume(*e, uses);
                }
            }
            Expr::Let { pat, expr: init } => self.walk_let(*pat, *init, uses),
            Expr::Match { scrutinee, arms } => {
                match self.capture_place(*scrutinee) {
                    Some(place) if self.reborrows.get(*scrutinee).is_none() => {
                        for arm in arms.iter() {
                            self.walk_pat(arm.pat, place.clone(), uses);
                        }
                    }
                    _ => self.consume(*scrutinee, uses),
                }
                for arm in arms.iter() {
                    if let Some(guard) = arm.guard {
                        self.consume(guard, uses);
                    }
                    self.consume(arm.body, uses);
                }
            }
            Expr::Loop { body: inner, .. } => self.consume(*inner, uses),
            Expr::Break { value, .. } | Expr::Return { value } => {
                if let Some(v) = value {
                    self.consume(*v, uses);
                }
            }
            Expr::Call { callee, args } => {
                match self.methods.get(expr).copied() {
                    Some(m) => self.adjusted(*callee, m.derefs, m.autoref, uses),
                    None if matches!(body.exprs[*callee], Expr::Path(_)) => {}
                    None => self.walk(*callee, uses),
                }
                for &arg in args.iter() {
                    self.consume(arg, uses);
                }
            }
            Expr::MethodCall { receiver, args, .. } => {
                if let Some(m) = self.methods.get(expr).copied() {
                    self.adjusted(*receiver, m.derefs, m.autoref, uses);
                } else if let Some(&(_, derefs)) = self.builtin_methods.get(expr) {
                    // `len` and `is_empty` of arrays and slices take `&self`.
                    self.adjusted(*receiver, derefs, Some(Mutability::Not), uses);
                } else {
                    self.consume(*receiver, uses);
                }
                for &arg in args.iter() {
                    self.consume(arg, uses);
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let overloaded = self.overloads.get(expr).is_some();
                match op {
                    BinaryOp::Assign(_) => {
                        self.borrow(*lhs, None, uses);
                        self.consume(*rhs, uses);
                    }
                    // `PartialEq::eq(&a, &b)`.
                    BinaryOp::Cmp(_) if overloaded && self.compared_refs.get(expr).is_none() => {
                        self.borrow(*lhs, Some(Mutability::Not), uses);
                        self.borrow(*rhs, Some(Mutability::Not), uses);
                    }
                    // References compared are passed on as they are, but a
                    // `&mut` is reborrowed rather than moved: `&*a` for
                    // `==`, and `PartialOrd::lt(&a, &mut *b)`, as in rustc.
                    BinaryOp::Cmp(cmp) if self.compared_refs.get(expr).is_some() => {
                        let ordered = !matches!(cmp, flux_hir::CmpOp::Eq | flux_hir::CmpOp::Ne);
                        if ordered && self.is_mut_ref(*lhs) {
                            self.borrow(*lhs, Some(Mutability::Not), uses);
                            self.adjusted(*rhs, 1, Some(Mutability::Mut), uses);
                        } else {
                            self.shared_ref(*lhs, uses);
                            self.shared_ref(*rhs, uses);
                        }
                    }
                    _ => {
                        self.consume(*lhs, uses);
                        self.consume(*rhs, uses);
                    }
                }
            }
            Expr::Ref {
                mutable,
                expr: inner,
            } => {
                let m = if *mutable {
                    Mutability::Mut
                } else {
                    Mutability::Not
                };
                self.borrow(*inner, Some(m), uses);
            }
            Expr::Closure { .. } => {
                // Analyzed already: what it captures from outside this
                // closure, it uses as it captures it.
                let captures = self
                    .closure_infos
                    .get(&expr)
                    .map(|info| info.captures.clone())
                    .unwrap_or_default();
                for capture in captures.iter() {
                    let kind = match capture.mode {
                        CaptureMode::ByRef(m) => UseKind::Borrow(m),
                        CaptureMode::ByValue if self.is_copy_ty(capture.ty) => UseKind::Copy,
                        CaptureMode::ByValue => UseKind::Move,
                    };
                    let range = capture.range;
                    uses.add((capture.binding, capture.projection.to_vec()), kind, range);
                }
            }
        }
    }

    /// `let pat = init`: a place matched by the pattern, or a value.
    fn walk_let(&mut self, pat: PatId, init: ExprId, uses: &mut Uses<'_>) {
        self.walk_indices(init, uses);
        match self.capture_place(init) {
            Some(place) if self.reborrows.get(init).is_none() => self.walk_pat(pat, place, uses),
            _ => self.consume(init, uses),
        }
    }

    /// Matching `place` with `pat`: what its bindings bind and what its
    /// tests read. The fields of an enum's variant are not captured
    /// apart from the enum, as in rustc.
    fn walk_pat(&mut self, pat: PatId, place: Place, uses: &mut Uses<'_>) {
        let body = self.body;
        let range = self.source_map.pats.get(pat).copied().unwrap_or_default();
        let (binding, mut proj) = place;
        let derefs = self.pat_derefs.get(pat).copied().unwrap_or(0);
        proj.extend((0..derefs).map(|_| CaptureProj::Deref));
        let place = (binding, proj);
        match &body.pats[pat] {
            Pat::Missing | Pat::Wild => {}
            Pat::Bind { binding, sub } => {
                self.bind_use(*binding, place.clone(), range, uses);
                if let Some(sub) = sub {
                    self.walk_pat(*sub, place, uses);
                }
            }
            Pat::Tuple { pats, rest } => {
                let count = self.place_field_count(&place);
                for (i, p) in positions(pats, *rest, count) {
                    let mut field = place.clone();
                    field.1.push(CaptureProj::Field(i));
                    self.walk_pat(p, field, uses);
                }
            }
            Pat::TupleStruct {
                strukt, pats, rest, ..
            } => {
                if self.items.structs[*strukt].is_enum() {
                    self.read_discriminant(*strukt, &place, range, uses);
                    for &p in pats.iter() {
                        self.walk_pat_whole(p, &place, uses);
                    }
                } else {
                    let count = self.place_field_count(&place);
                    for (i, p) in positions(pats, *rest, count) {
                        let mut field = place.clone();
                        field.1.push(CaptureProj::Field(i));
                        self.walk_pat(p, field, uses);
                    }
                }
            }
            Pat::Record {
                strukt,
                variant,
                fields,
                ..
            } => {
                if self.items.structs[*strukt].is_enum() {
                    self.read_discriminant(*strukt, &place, range, uses);
                    for &(_, p) in fields.iter() {
                        self.walk_pat_whole(p, &place, uses);
                    }
                } else {
                    for &(name, p) in fields.iter() {
                        let Some(i) = field_index(self.items, *strukt, *variant, name) else {
                            continue;
                        };
                        let mut field = place.clone();
                        field.1.push(CaptureProj::Field(i));
                        self.walk_pat(p, field, uses);
                    }
                }
            }
            Pat::Unit(strukt, _) => {
                if self.items.structs[*strukt].is_enum() {
                    self.read_discriminant(*strukt, &place, range, uses);
                }
            }
            Pat::Expr(_) | Pat::Range { .. } => {
                uses.add(place, UseKind::Borrow(Mutability::Not), range);
            }
            Pat::Or(alternatives) => {
                for &alt in alternatives.iter() {
                    self.walk_pat(alt, place.clone(), uses);
                }
            }
            Pat::Ref { pat: inner, .. } => {
                let mut deref = place;
                deref.1.push(CaptureProj::Deref);
                self.walk_pat(*inner, deref, uses);
            }
        }
    }

    /// Testing which variant of enum `strukt` `place` holds reads it, if
    /// it has more than one.
    fn read_discriminant(
        &mut self,
        strukt: StructId,
        place: &Place,
        range: TextRange,
        uses: &mut Uses<'_>,
    ) {
        let variants = self.items.structs[strukt]
            .variants
            .as_ref()
            .map_or(0, |v| v.len());
        if variants > 1 {
            uses.add(place.clone(), UseKind::Borrow(Mutability::Not), range);
        }
    }

    /// A pattern inside an enum's variant at `place`: its uses are of
    /// the whole enum.
    fn walk_pat_whole(&mut self, pat: PatId, place: &Place, uses: &mut Uses<'_>) {
        let body = self.body;
        let range = self.source_map.pats.get(pat).copied().unwrap_or_default();
        match &body.pats[pat] {
            Pat::Missing | Pat::Wild => {}
            Pat::Bind { binding, sub } => {
                let kind = match self.binding_refs.get(*binding) {
                    Some(&m) => UseKind::Borrow(m),
                    None => {
                        let ty = self.known_binding_ty(*binding);
                        if self.is_copy_ty(ty) {
                            UseKind::Copy
                        } else {
                            UseKind::Move
                        }
                    }
                };
                uses.add(place.clone(), kind, range);
                if let Some(sub) = sub {
                    self.walk_pat_whole(*sub, place, uses);
                }
            }
            Pat::Expr(_) | Pat::Range { .. } | Pat::Unit(..) => {
                uses.add(place.clone(), UseKind::Borrow(Mutability::Not), range);
            }
            other => {
                for child in other.children().collect::<Vec<_>>() {
                    self.walk_pat_whole(child, place, uses);
                }
            }
        }
    }

    /// A binding binding `place`: a borrow of it, or its value.
    fn bind_use(
        &mut self,
        binding: BindingId,
        place: Place,
        range: TextRange,
        uses: &mut Uses<'_>,
    ) {
        let kind = match self.binding_refs.get(binding) {
            Some(&m) => UseKind::Borrow(m),
            None => {
                let ty = self.capture_place_ty(place.0, &place.1);
                if self.is_copy_ty(ty) {
                    UseKind::Copy
                } else {
                    UseKind::Move
                }
            }
        };
        uses.add(place, kind, range);
    }

    fn place_field_count(&mut self, place: &Place) -> usize {
        let ty = self.capture_place_ty(place.0, &place.1);
        self.sigs.fields(ty).map_or(0, <[Ty]>::len)
    }

    /// For the solver: whether `pred`, on a closure's type, holds: a
    /// `Fn`-family trait by its kind and signature, `Copy` and `Clone`
    /// by what it captures, once that is known.
    pub(super) fn select_closure(&mut self, pred: Predicate) -> super::solve::Selection {
        use super::solve::Selection as ClosureSelection;
        let TyKind::Closure(_, args) = self.shallow(pred.self_ty).kind() else {
            return ClosureSelection::Unmet;
        };
        let closure = self.shallow(pred.self_ty);
        match crate::traits::closure_goal(self.items, pred.trait_ref.trait_, args) {
            crate::traits::ClosureGoal::Fn(kind, _) => {
                let params = pred.trait_ref.args.first().copied().unwrap_or(Ty::ERROR);
                if self.select_closure_fn(closure, kind, params) {
                    ClosureSelection::Met(Vec::new())
                } else {
                    ClosureSelection::Unmet
                }
            }
            crate::traits::ClosureGoal::Upvars(_) => {
                let upvars = self.shallow(ClosureArgs::split(args).upvars);
                match upvars.kind() {
                    TyKind::Tuple(list) => {
                        let trait_ref =
                            crate::traits::copy_trait_ref(self.items).unwrap_or(pred.trait_ref);
                        ClosureSelection::Met(
                            list.iter()
                                .map(|&ty| Predicate {
                                    self_ty: ty,
                                    trait_ref,
                                })
                                .collect(),
                        )
                    }
                    _ => ClosureSelection::Ambiguous,
                }
            }
            crate::traits::ClosureGoal::No => ClosureSelection::Unmet,
        }
    }
}

/// Inserts a capture of `place` in `mode`, merged with those of places
/// it contains or that contain it: the outermost place is captured, in
/// the strongest mode.
type PendingCapture = (Place, CaptureMode, TextRange, bool);

fn insert_capture(captures: &mut Vec<PendingCapture>, new: PendingCapture) {
    let is_prefix = |a: &Place, b: &Place| a.0 == b.0 && b.1.starts_with(&a.1);
    // The stronger of two uses decides, and where it is reported.
    let merge = |into: &mut PendingCapture, other: &PendingCapture| {
        if other.1.rank() > into.1.rank() {
            into.1 = other.1;
            into.2 = other.2;
            into.3 = other.3;
        }
    };
    if let Some(c) = captures.iter_mut().find(|c| is_prefix(&c.0, &new.0)) {
        merge(c, &new);
        return;
    }
    let mut new = new;
    let mut at = None;
    let mut i = 0;
    while i < captures.len() {
        if is_prefix(&new.0, &captures[i].0) {
            let old = captures.remove(i);
            // The first use comes first.
            if old.2.start() < new.2.start() && old.1.rank() >= new.1.rank() {
                new.2 = old.2;
                new.3 = old.3;
            }
            merge(&mut new, &old);
            at.get_or_insert(i);
        } else {
            i += 1;
        }
    }
    let at = at.unwrap_or(captures.len());
    captures.insert(at, new);
}

/// The patterns of a tuple pattern with the fields they match.
fn positions(pats: &[PatId], rest: Option<u32>, count: usize) -> Vec<(u32, PatId)> {
    pats.iter()
        .enumerate()
        .map(|(i, &p)| {
            let index = match rest {
                Some(rest) if i >= rest as usize => count.saturating_sub(pats.len() - i),
                _ => i,
            };
            (index as u32, p)
        })
        .collect()
}

fn field_index(
    items: &flux_hir::ItemTree,
    strukt: StructId,
    variant: u32,
    name: Symbol,
) -> Option<u32> {
    items.structs[strukt]
        .variant(variant)
        .field(name)
        .map(|(i, _)| i as u32)
}

fn pat_bindings_into(body: &flux_hir::Body, pat: PatId, out: &mut HashSet<BindingId>) {
    if let Pat::Bind { binding, .. } = body.pats[pat] {
        out.insert(binding);
    }
    for child in body.pats[pat].children() {
        pat_bindings_into(body, child, out);
    }
}

/// Every binding the patterns in `expr` bind, nested closures' included.
fn expr_bindings(body: &flux_hir::Body, expr: ExprId, out: &mut HashSet<BindingId>) {
    let mut stack = vec![expr];
    while let Some(e) = stack.pop() {
        match &body.exprs[e] {
            Expr::Missing | Expr::Literal(_) | Expr::Path(_) | Expr::Continue { .. } => {}
            Expr::Tuple(list) | Expr::Array(list) => stack.extend(list.iter().copied()),
            Expr::Repeat { value, .. } => stack.push(*value),
            Expr::Index { base, index } => {
                stack.push(*base);
                stack.push(*index);
            }
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts.iter() {
                    match *stmt {
                        Stmt::Let {
                            pat,
                            init,
                            else_branch,
                            ..
                        } => {
                            pat_bindings_into(body, pat, out);
                            stack.extend(init);
                            stack.extend(else_branch);
                        }
                        Stmt::Expr { expr, .. } => stack.push(expr),
                    }
                }
                stack.extend(*tail);
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                stack.push(*condition);
                stack.push(*then_branch);
                stack.extend(*else_branch);
            }
            Expr::Let { pat, expr } => {
                pat_bindings_into(body, *pat, out);
                stack.push(*expr);
            }
            Expr::Match { scrutinee, arms } => {
                stack.push(*scrutinee);
                for arm in arms.iter() {
                    pat_bindings_into(body, arm.pat, out);
                    stack.extend(arm.guard);
                    stack.push(arm.body);
                }
            }
            Expr::Loop { body: inner, .. } => stack.push(*inner),
            Expr::Break { value, .. } | Expr::Return { value } => stack.extend(*value),
            Expr::Call { callee, args } => {
                stack.push(*callee);
                stack.extend(args.iter().copied());
            }
            Expr::MethodCall { receiver, args, .. } => {
                stack.push(*receiver);
                stack.extend(args.iter().copied());
            }
            Expr::Unary { operand, .. } => stack.push(*operand),
            Expr::Binary { lhs, rhs, .. } => {
                stack.push(*lhs);
                stack.push(*rhs);
            }
            Expr::Cast { expr, .. }
            | Expr::Try { expr }
            | Expr::Field { expr, .. }
            | Expr::Ref { expr, .. } => stack.push(*expr),
            Expr::Record { fields, spread, .. } => {
                stack.extend(fields.iter().map(|&(_, f)| f));
                stack.extend(*spread);
            }
            Expr::Closure {
                params, body: b, ..
            } => {
                for &p in params.iter() {
                    pat_bindings_into(body, p, out);
                }
                stack.push(*b);
            }
        }
    }
}
