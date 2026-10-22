//! Traits in a body: the obligations calls create, selecting the `impl`
//! block or bound that meets each, and the methods and associated
//! functions found through traits.
//!
//! An obligation `ty: Tr` is met by a bound in scope (for generic
//! parameters) or by the one `impl` block of `Tr` whose header unifies
//! with it, whose own bounds become obligations in turn. While `ty` is an
//! unknown variable, or more than one block may apply, it waits; at the
//! end, integer variables take their default and what is left is
//! reported.

use std::collections::HashSet;

use flux_hir::{AssocItem, ExprId, FunctionId, ImplId, Owner, TraitId};
use flux_intern::Symbol;
use flux_syntax::TextRange;

use super::Ctx;
use crate::Mutability;
use crate::traits::{Predicate, TraitRef, owner_param_count, predicates_of};
use crate::ty::{AliasTy, IntTy, Ty, TyKind};

#[derive(Debug, Clone, Copy)]
pub(super) struct Obligation {
    pub pred: Predicate,
    /// The obligation it came from through `impl` blocks' bounds, which
    /// rustc names when it is not met.
    pub root: Predicate,
    pub range: TextRange,
    pub kind: ObligationKind,
    /// How many `impl` blocks' bounds it is from `root`.
    pub depth: u32,
}

/// How an unmet obligation is reported.
#[derive(Debug, Clone, Copy)]
pub(super) enum ObligationKind {
    /// "the trait bound `S: Tr` is not satisfied"
    Bound,
    /// A bound of the block an item was found in, on a type known when it
    /// was: "the method `m` exists for struct `W<S>`, but its trait
    /// bounds were not satisfied". `what` is "method" or "associated
    /// function or constant".
    ItemBounds {
        what: &'static str,
        name: Symbol,
        ty: Ty,
    },
    /// `Self: Tr` for `Tr::f()` or `Tr::N`, whose `Self` must become
    /// known; `range` is the call's or path's, where rustc reports it if
    /// it does not.
    TraitCall { range: TextRange, constant: bool },
    /// An operator's trait, for operands of these types: "cannot add `S`
    /// to `S`".
    Operator { op: Op, lhs: Ty, rhs: Ty },
    /// `F: From<E>` for `?` on a `Result<T, E>` in a function returning
    /// `Result<U, F>`: "`?` couldn't convert the error to `F`".
    QuestionMark { target: Ty },
}

/// An operator, for messages.
#[derive(Debug, Clone, Copy)]
pub(super) enum Op {
    Binary(flux_hir::BinaryOp),
    Unary(flux_hir::UnaryOp),
}

/// An associated type whose `Self` was not known when it was met: `var`
/// stands for it until it is. If `expected` is set (from a bound's
/// `Tr<Out = u8>`), a mismatch is rustc's E0271 at `range`.
#[derive(Debug, Clone, Copy)]
pub(super) struct PendingProjection {
    pub alias: AliasTy,
    pub var: Ty,
    pub range: TextRange,
    pub expected: bool,
}

pub(super) enum Selection {
    /// Met, if these are.
    Met(Vec<Predicate>),
    Ambiguous,
    Unmet,
}

/// Where a call is, for the spans of its obligations.
#[derive(Debug, Clone, Copy)]
pub(super) struct CallSite<'s> {
    /// The callee path, or the method's name.
    pub callee: TextRange,
    pub whole: TextRange,
    /// The receiver of a method call, the first argument.
    pub receiver: Option<ExprId>,
    pub args: &'s [ExprId],
    /// `<S as Tr>`'s `S`, where a failed `Self: Tr` is reported.
    pub qself: Option<TextRange>,
    /// The expression whose generic arguments were written.
    pub expr: ExprId,
}

impl<'a> Ctx<'a> {
    /// The signature of `function` called with generic arguments `args`,
    /// its associated types normalized.
    pub(super) fn instance_sig(
        &mut self,
        function: FunctionId,
        args: &[Ty],
    ) -> std::borrow::Cow<'a, crate::FnSig> {
        let sigs = self.sigs;
        let sig = &sigs[function];
        let has_aliases = sig.ret.has_aliases()
            || sig.params.iter().any(|p| p.has_aliases())
            || (Some(function) == self.owner_fn && !self.own_opaques.is_empty());
        if args.is_empty() && !has_aliases {
            return std::borrow::Cow::Borrowed(sig);
        }
        let sig = sig.subst(args);
        if !has_aliases {
            return std::borrow::Cow::Owned(sig);
        }
        let origin = TextRange::default();
        std::borrow::Cow::Owned(crate::FnSig {
            params: sig
                .params
                .iter()
                .map(|&p| self.normalize(p, origin))
                .collect(),
            ret: self.normalize(sig.ret, origin),
        })
    }
}

impl Ctx<'_> {
    /// The bounds of each struct `ty` names, which a type at `range` must
    /// meet; `written` if it is written there. As in rustc, a struct type
    /// a written type already needed is not checked again, nor in a
    /// constant's value one its type names.
    pub(super) fn register_wf(&mut self, ty: Ty, range: TextRange, written: bool) {
        match ty.kind() {
            TyKind::Adt(adt, args) => {
                for &arg in args {
                    self.register_wf(arg, range, written);
                }
                let known = self.known(ty);
                if written && !self.written_wf.insert(known)
                    || !written && self.signature_wf.contains(&known)
                {
                    return;
                }
                let preds = self.sigs.struct_predicates.get(adt.strukt);
                for p in preds.into_iter().flatten() {
                    self.register(p.subst(args), range, ObligationKind::Bound);
                }
                let projections = self.sigs.struct_projections.get(adt.strukt);
                for p in projections.into_iter().flatten() {
                    self.register_projection(p.subst(args), range);
                }
            }
            TyKind::Tuple(elements) => {
                for &e in elements {
                    self.register_wf(e, range, written);
                }
            }
            TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                self.register_wf(t, range, written)
            }
            TyKind::FnPtr(sig) => {
                for &t in sig.inputs_output {
                    self.register_wf(t, range, written);
                }
            }
            _ => {}
        }
    }

    /// Notes the struct types `ty`, a constant's type, names: what is
    /// reported with the constant is not reported again in its value.
    pub(super) fn signature_wf(&mut self, ty: Ty) {
        match ty.kind() {
            TyKind::Adt(_, args) => {
                for &arg in args {
                    self.signature_wf(arg);
                }
                self.written_wf.insert(ty);
                self.signature_wf.insert(ty);
            }
            TyKind::Tuple(elements) => elements.iter().for_each(|&e| self.signature_wf(e)),
            TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                self.signature_wf(t)
            }
            TyKind::FnPtr(sig) => sig.inputs_output.iter().for_each(|&t| self.signature_wf(t)),
            _ => {}
        }
    }

    /// The bounds of struct `ty`, built by a constructor or struct
    /// expression whose field values are `values` (by field index): each
    /// where rustc reports it, at the one value whose field's type has
    /// the bound's parameter, else at `fallback`.
    pub(super) fn register_struct_bounds(
        &mut self,
        ty: Ty,
        values: &[Option<ExprId>],
        fallback: TextRange,
    ) {
        let TyKind::Adt(adt, args) = ty.kind() else {
            return;
        };
        let sigs = self.sigs;
        let fields = sigs.structs[adt.strukt];
        let range = |this: &Self, self_ty: Ty| {
            let Some(param) = first_param(self_ty) else {
                return fallback;
            };
            let mut found = fields
                .iter()
                .enumerate()
                .filter(|(_, f)| mentions(**f, param));
            match (found.next(), found.next()) {
                (Some((i, _)), None) => match values.get(i) {
                    Some(Some(value)) => this.range(*value),
                    _ => fallback,
                },
                _ => fallback,
            }
        };
        for p in sigs.struct_predicates.get(adt.strukt).into_iter().flatten() {
            let at = range(self, p.self_ty);
            self.register(p.subst(args), at, ObligationKind::Bound);
        }
        for p in sigs
            .struct_projections
            .get(adt.strukt)
            .into_iter()
            .flatten()
        {
            let at = range(self, p.alias.args[0]);
            self.register_projection(p.subst(args), at);
        }
    }

    /// What a bound says an associated type is (`S: Shape<Out = i32>`),
    /// at `range` if it is not.
    pub(super) fn register_projection(&mut self, p: crate::traits::Projection, range: TextRange) {
        let expected = self.normalize(p.ty, range);
        let var = self.new_var(false, range);
        self.unify(var, expected);
        self.projections.push(PendingProjection {
            alias: p.alias,
            var,
            range,
            expected: true,
        });
    }

    /// `pred` with its associated types normalized: `<S as Tr>::Out:
    /// Marker` is about what `Out` is.
    fn normalize_predicate(&mut self, pred: Predicate, range: TextRange) -> Predicate {
        if !pred.self_ty.has_aliases() && !pred.trait_ref.args.iter().any(|a| a.has_aliases()) {
            return pred;
        }
        let args: Vec<Ty> = pred
            .trait_ref
            .args
            .iter()
            .map(|&a| self.normalize(a, range))
            .collect();
        Predicate {
            self_ty: self.normalize(pred.self_ty, range),
            trait_ref: TraitRef {
                args: Ty::list(&args),
                ..pred.trait_ref
            },
        }
    }

    pub(super) fn register(&mut self, pred: Predicate, range: TextRange, kind: ObligationKind) {
        let pred = self.normalize_predicate(pred, range);
        self.obligations.push(Obligation {
            pred,
            root: pred,
            range,
            kind,
            depth: 0,
        });
    }

    /// Whether `f` holds, leaving the variables as they were.
    pub(super) fn probe(&mut self, f: impl FnOnce(&mut Self) -> bool) -> bool {
        let snapshot = self.vars.clone();
        let projections = self.projections.len();
        let result = f(self);
        self.vars = snapshot;
        self.projections.truncate(projections);
        result
    }

    /// Selects what can be, until nothing changes, reporting obligations
    /// that cannot be met.
    pub(super) fn select_obligations(&mut self) {
        loop {
            let mut progress = self.select_projections();
            let pending = std::mem::take(&mut self.obligations);
            for ob in pending {
                match self.select_predicate(ob.pred) {
                    Selection::Met(nested) => {
                        progress = true;
                        let kind = match ob.kind {
                            ObligationKind::TraitCall { .. } => ObligationKind::Bound,
                            kind => kind,
                        };
                        // A bound that needs itself, or ever larger types,
                        // overflows, as in rustc.
                        if ob.depth > 128 && !nested.is_empty() {
                            let root = self.describe_predicate(ob.root);
                            self.error(
                                ob.range,
                                format!("overflow evaluating the requirement `{root}`"),
                            );
                            continue;
                        }
                        for pred in nested {
                            let pred = self.normalize_predicate(pred, ob.range);
                            self.obligations.push(Obligation {
                                pred,
                                kind,
                                depth: ob.depth + 1,
                                ..ob
                            });
                        }
                    }
                    Selection::Ambiguous => self.obligations.push(ob),
                    Selection::Unmet => {
                        progress = true;
                        self.report_unmet(ob);
                    }
                }
            }
            if !progress {
                break;
            }
        }
    }

    /// Replaces the variables of pending associated types whose `Self` is
    /// now known; whether any was.
    fn select_projections(&mut self) -> bool {
        let pending = std::mem::take(&mut self.projections);
        let mut progress = false;
        for p in pending {
            let args: Vec<Ty> = p.alias.args.iter().map(|&a| self.shallow(a)).collect();
            let alias = AliasTy {
                args: Ty::list(&args),
                ..p.alias
            };
            // Known, without an `impl` block or a bound that says what
            // it is: an associated type of a generic parameter, rigid.
            let found = self.try_normalize(alias).or_else(|| {
                (!self.has_unknown(Ty::alias(alias)))
                    .then(|| self.normalize(Ty::alias(alias), p.range))
            });
            match found {
                Some(ty) => {
                    progress = true;
                    let ty = self.normalize(ty, p.range);
                    if !self.unify(p.var, ty) {
                        let iterator_item = self.items.traits[alias.trait_]
                            .lang
                            .is_some_and(|l| l.as_str() == "iterator")
                            && alias.name.as_str() == "Item";
                        let message = if iterator_item {
                            format!(
                                "expected `{}` to be an iterator that yields `{}`, but it yields `{}`",
                                self.describe(alias.args[0]),
                                self.describe(p.var),
                                self.describe(ty)
                            )
                        } else {
                            format!(
                                "type mismatch resolving `{} == {}`",
                                self.describe(Ty::alias(alias)),
                                self.describe(p.var)
                            )
                        };
                        if p.expected {
                            self.error(p.range, message);
                        } else {
                            self.mismatch(p.var, ty, p.range);
                        }
                    }
                }
                None => self.projections.push(PendingProjection { alias, ..p }),
            }
        }
        progress
    }

    /// `ty` with each associated type in it replaced by what it is, where
    /// that is known: from the bounds in scope or the `impl` block for its
    /// types. One whose `Self` is not known yet becomes a variable;
    /// one of a generic parameter without a binding stays.
    pub(super) fn normalize(&mut self, ty: Ty, origin: TextRange) -> Ty {
        let ty = self.reveal_own(ty);
        if !ty.has_aliases() {
            return ty;
        }
        match ty.kind() {
            TyKind::Alias(alias) => {
                // Known variables replaced, so that an alias met again
                // through an `impl` block's fresh variables is the same.
                let args: Vec<Ty> = alias
                    .args
                    .iter()
                    .map(|&a| {
                        let a = self.normalize(a, origin);
                        self.known(a)
                    })
                    .collect();
                let alias = AliasTy {
                    args: Ty::list(&args),
                    ..alias
                };
                if self.normalizing.contains(&alias) {
                    let message = format!(
                        "overflow evaluating the requirement `{} == _`",
                        Ty::alias(alias)
                    );
                    if !self.diagnostics.iter().any(|d| d.message == message) {
                        self.error(origin, message);
                    }
                    return Ty::ERROR;
                }
                if let Some(ty) = self.try_normalize(alias) {
                    self.normalizing.push(alias);
                    let normalized = self.normalize(ty, origin);
                    self.normalizing.pop();
                    return normalized;
                }
                let self_ty = self.shallow(args[0]);
                // A known type that does not implement the trait
                // (reported where it is written).
                if !self_ty.has_params() && !self_ty.has_aliases() && !self.has_unknown(self_ty) {
                    return Ty::ERROR;
                }
                if self.has_unknown(self_ty) {
                    let var = self.new_var(false, origin);
                    self.projections.push(PendingProjection {
                        alias,
                        var,
                        range: origin,
                        expected: false,
                    });
                    return var;
                }
                Ty::alias(alias)
            }
            TyKind::Tuple(list) => {
                let list: Vec<Ty> = list.iter().map(|&t| self.normalize(t, origin)).collect();
                Ty::tuple(&list)
            }
            TyKind::Adt(adt, list) => {
                let list: Vec<Ty> = list.iter().map(|&t| self.normalize(t, origin)).collect();
                Ty::adt(adt.strukt, adt.name, &list)
            }
            TyKind::Closure(id, list) => {
                let list: Vec<Ty> = list.iter().map(|&t| self.normalize(t, origin)).collect();
                Ty::closure(id, &list)
            }
            TyKind::Ref(m, t) => Ty::new(TyKind::Ref(m, self.normalize(t, origin))),
            TyKind::RawPtr(m, t) => Ty::raw_ptr(m, self.normalize(t, origin)),
            TyKind::Array(t, len) => Ty::array(self.normalize(t, origin), len),
            TyKind::Slice(t) => Ty::slice(self.normalize(t, origin)),
            TyKind::FnDef(_) | TyKind::FnPtr(_) => ty.map_fn(|t| self.normalize(t, origin)),
            _ => ty,
        }
    }

    /// `ty`, shallowly resolved, with what a reference points to
    /// resolved too: enough to tell `&[T; N]`.
    pub(super) fn shallow_ref(&mut self, ty: Ty) -> Ty {
        match self.shallow(ty).kind() {
            TyKind::Ref(m, pointee) => Ty::new(TyKind::Ref(m, self.shallow(pointee))),
            _ => self.shallow(ty),
        }
    }

    /// What `alias` is, if a bound in scope or one `impl` block says.
    fn try_normalize(&mut self, alias: AliasTy) -> Option<Ty> {
        // A `Self` not known yet is ambiguous: a bound in scope must not
        // decide what it is.
        if let TyKind::Infer(var) = self.shallow(alias.args[0]).kind()
            && !self.unknown(var).0
        {
            return None;
        }
        let target = Ty::alias(alias);
        let mut candidates = self.env_projections.clone();
        match self.shallow(alias.args[0]).kind() {
            TyKind::Alias(outer) => {
                candidates.extend(crate::traits::alias_projections(self.sigs, outer));
            }
            TyKind::Opaque(opaque) => {
                candidates.extend(crate::traits::opaque_projections(self.sigs, opaque));
            }
            _ => {}
        }
        for p in candidates {
            if p.alias.trait_ == alias.trait_
                && p.alias.name == alias.name
                && self.probe(|cx| cx.unify(Ty::alias(p.alias), target))
            {
                self.unify(Ty::alias(p.alias), target);
                return Some(p.ty);
            }
        }
        let self_ty = self.shallow(alias.args[0]);
        if let TyKind::Infer(var) = self_ty.kind()
            && !self.unknown(var).0
        {
            return None;
        }
        if matches!(
            self_ty.kind(),
            TyKind::Closure(..) | TyKind::FnDef(_) | TyKind::FnPtr(_)
        ) {
            let alias = AliasTy {
                args: Ty::list(&[&[self_ty][..], &alias.args[1..]].concat()),
                ..alias
            };
            return crate::traits::closure_output(self.items, alias);
        }
        let self_ref = self.shallow_ref(self_ty);
        if let Some(ty) = crate::traits::array_into_iter_type(
            self.items,
            AliasTy {
                args: Ty::list(&[&[self_ref][..], &alias.args[1..]].concat()),
                ..alias
            },
        ) {
            return Some(ty);
        }
        let pred = Predicate {
            self_ty,
            trait_ref: TraitRef {
                trait_: alias.trait_,
                name: alias.trait_name,
                args: Ty::list(&alias.args[1..]),
            },
        };
        // A type parameter (or rigid associated type) that a bound in
        // scope says implements the trait: the bound decides, as rustc's
        // where-clause candidates shadow `impl` blocks, so a blanket
        // block does not apply to it.
        if matches!(
            self_ty.kind(),
            TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_)
        ) {
            let alias_bounds = self.alias_bounds(self_ty);
            if self
                .param_env
                .iter()
                .chain(&alias_bounds)
                .any(|p| p.trait_ref.trait_ == alias.trait_ && p.self_ty == self_ty)
            {
                return None;
            }
        }
        let (items, sigs) = (self.items, self.sigs);
        let impls = sigs.impl_index.impls_for(items, alias.trait_, self_ty);
        let found: Vec<ImplId> = impls
            .filter(|&imp| self.probe(|cx| cx.unify_impl(imp, pred).is_some()))
            .collect();
        let found = if found.len() > 1 {
            self.winnow(found, pred)
        } else {
            found
        };
        let [imp] = found.as_slice() else {
            return None;
        };
        let args = self.unify_impl(*imp, pred)?;
        self.register_impl_projections(*imp, &args);
        let ty = crate::traits::impl_type(self.sigs, *imp, alias.name)?;
        Some(ty.subst(&args))
    }

    /// What the bounds of `impl` block `imp`, used with generic arguments
    /// `args`, say associated types are: `B` in `impl<B, I, F> Iterator
    /// for Map<I, F> where F: FnMut(I::Item) -> B` is what `F` returns.
    fn register_impl_projections(&mut self, imp: ImplId, args: &[Ty]) {
        let projections = self.sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
        if projections.is_empty() {
            return;
        }
        for &p in projections {
            self.register_projection(p.subst(args), TextRange::default());
        }
    }

    /// For a type written in a body as `written`, at `range`,
    /// `<S as Tr>::Out` needs `S: Tr`, which rustc reports at `S`.
    pub(super) fn require_alias_traits(
        &mut self,
        ty: Ty,
        written: Option<&flux_hir::TypeRef>,
        range: TextRange,
    ) {
        use flux_hir::TypeRef;
        match ty.kind() {
            TyKind::Alias(alias) => {
                let pred = Predicate {
                    self_ty: alias.args[0],
                    trait_ref: TraitRef {
                        trait_: alias.trait_,
                        name: alias.trait_name,
                        args: Ty::list(&alias.args[1..]),
                    },
                };
                let qself = match written {
                    Some(TypeRef::Path(path)) => path.qself.as_deref(),
                    _ => None,
                };
                self.register(
                    pred,
                    qself.map_or(range, |q| q.ty_range),
                    ObligationKind::Bound,
                );
                let trait_args = qself
                    .and_then(|q| q.trait_.as_ref()?.args.last()?.as_ref())
                    .map_or(&[][..], |a| &a.types);
                for (i, &a) in alias.args.iter().enumerate() {
                    let part = match i {
                        0 => qself.map(|q| &q.ty),
                        _ => trait_args.get(i - 1),
                    };
                    self.require_alias_traits(a, part, range);
                }
            }
            TyKind::Tuple(list) | TyKind::Adt(_, list) => {
                let parts = match written {
                    Some(TypeRef::Tuple(elements)) => &elements[..],
                    Some(TypeRef::Path(path)) => path
                        .args
                        .last()
                        .and_then(Option::as_ref)
                        .map_or(&[][..], |a| &a.types),
                    _ => &[],
                };
                for (i, &t) in list.iter().enumerate() {
                    self.require_alias_traits(t, parts.get(i), range);
                }
            }
            TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                let pointee = match written {
                    Some(
                        TypeRef::Ref { pointee, .. }
                        | TypeRef::Ptr { pointee, .. }
                        | TypeRef::Array { elem: pointee, .. }
                        | TypeRef::Slice(pointee),
                    ) => Some(&**pointee),
                    _ => None,
                };
                self.require_alias_traits(t, pointee, range);
            }
            TyKind::FnPtr(sig) => {
                for &t in sig.inputs_output {
                    self.require_alias_traits(t, None, range);
                }
            }
            _ => {}
        }
    }

    /// Once the body is inferred: integer variables in waiting
    /// obligations take their default, and what still waits is
    /// reported.
    pub(super) fn finish_obligations(&mut self, reported: &mut HashSet<crate::ty::InferVar>) {
        self.select_obligations();
        let pending = self.obligations.clone();
        for ob in &pending {
            let mut vars = Vec::new();
            self.int_vars(ob.pred.self_ty, &mut vars);
            for var in vars {
                self.vars[var.0 as usize].0 = super::VarValue::Known(Ty::int(IntTy::I32));
            }
        }
        self.select_obligations();
        // As in rustc, a body with errors, or with values of types that
        // are (from its signature or a struct's), reports no ambiguity:
        // what is unknown is likely so because of them.
        let tainted = !self.diagnostics.is_empty()
            || self.expr_types.iter().any(|(_, &t)| t == Ty::ERROR)
            || self.pat_types.iter().any(|(_, &t)| t == Ty::ERROR);
        self.tainted = tainted;
        for ob in std::mem::take(&mut self.obligations) {
            if tainted {
                continue;
            }
            let self_ty = self.shallow(ob.pred.self_ty);
            let mut unknown = Vec::new();
            self.unknown_vars(self_ty, &mut unknown);
            if unknown.iter().any(|v| reported.contains(v)) {
                continue;
            }
            reported.extend(unknown.iter().copied());
            match ob.kind {
                ObligationKind::TraitCall { range, constant }
                    if matches!(self_ty.kind(), TyKind::Infer(_)) =>
                {
                    let message = if constant {
                        "cannot refer to the associated constant on trait without specifying the corresponding `impl` type"
                    } else {
                        "cannot call associated function on trait without specifying the corresponding `impl` type"
                    };
                    self.error(range, message);
                }
                _ => self.error(ob.range, "type annotations needed"),
            }
        }
    }

    fn int_vars(&mut self, ty: Ty, out: &mut Vec<crate::ty::InferVar>) {
        let ty = self.shallow(ty);
        match ty.kind() {
            TyKind::Infer(var) if self.unknown(var).0 => out.push(self.root(var)),
            TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => {
                for &t in list {
                    self.int_vars(t, out);
                }
            }
            TyKind::Alias(alias) => {
                for &t in alias.args {
                    self.int_vars(t, out);
                }
            }
            TyKind::Opaque(opaque) => {
                for &t in opaque.args {
                    self.int_vars(t, out);
                }
            }
            TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                self.int_vars(t, out)
            }
            TyKind::FnDef(_) | TyKind::FnPtr(_) => {
                for t in ty.children() {
                    self.int_vars(t, out);
                }
            }
            _ => {}
        }
    }

    fn report_unmet(&mut self, ob: Obligation) {
        let sized =
            self.items.traits[ob.root.trait_ref.trait_].lang == Some(flux_intern::sym::sized);
        let message = match ob.kind {
            _ if sized => format!(
                "the size for values of type `{}` cannot be known at compilation time",
                self.describe(ob.root.self_ty)
            ),
            ObligationKind::Operator { op, lhs, rhs } => {
                let (lhs, rhs) = (self.describe(lhs), self.describe(rhs));
                match op {
                    Op::Binary(op) => super::binary_error(op, &lhs, &rhs),
                    Op::Unary(op) => {
                        let text = if op == flux_hir::UnaryOp::Neg {
                            "-"
                        } else {
                            "!"
                        };
                        format!("cannot apply unary operator `{text}` to type `{lhs}`")
                    }
                }
            }
            ObligationKind::QuestionMark { target } => {
                format!(
                    "`?` couldn't convert the error to `{}`",
                    self.describe(target)
                )
            }
            ObligationKind::ItemBounds { what, name, ty } => {
                format!(
                    "the {what} `{name}` exists for {}, but its trait bounds were not satisfied",
                    self.describe_kind(ty)
                )
            }
            _ => {
                let path = self.trait_path(ob.root.trait_ref.trait_);
                crate::traits::unmet_message(self.items, ob.root, &mut |t| self.describe(t), &path)
            }
        };
        if self.reported_unmet.insert((ob.range, message.clone())) {
            self.error(ob.range, message);
        }
    }

    /// `S: Tr<A>`, as rustc shows a bound in messages.
    fn describe_predicate(&mut self, p: Predicate) -> String {
        let args: Vec<String> = p.trait_ref.args.iter().map(|&a| self.describe(a)).collect();
        let args = if args.is_empty() {
            String::new()
        } else {
            format!("<{}>", args.join(", "))
        };
        let path = self.trait_path(p.trait_ref.trait_);
        format!("{}: {path}{args}", self.describe(p.self_ty))
    }

    /// A trait as rustc names it in messages: by its path from the crate
    /// root (`m::Tr`) if it is the program's, else by its name.
    fn trait_path(&self, t: TraitId) -> String {
        let items = self.items;
        let data = &items.traits[t];
        let module = data.loc.module;
        if !items.in_main_crate(module) || items.modules[module].parent.is_none() {
            return data.name.to_string();
        }
        let path = items.module_path(module);
        let path = path.strip_prefix("crate::").unwrap_or(&path);
        format!("{path}::{}", data.name)
    }

    /// "struct `S`", "reference `&S`", "type parameter `T`", ...: a type
    /// as rustc's method errors name it.
    pub(super) fn describe_kind(&mut self, ty: Ty) -> String {
        let kind = match self.shallow(ty).kind() {
            TyKind::Adt(adt, _) if self.sigs.is_enum(adt.strukt) => "enum",
            TyKind::Adt(..) => "struct",
            TyKind::Ref(..) => "reference",
            TyKind::Tuple(_) => "tuple",
            TyKind::Param(_) => "type parameter",
            TyKind::Opaque(_) => "opaque type",
            TyKind::RawPtr(..) => "raw pointer",
            TyKind::Array(..) => "array",
            TyKind::Slice(_) => "slice",
            _ => "type",
        };
        format!("{kind} `{}`", self.describe(ty))
    }

    /// The bounds associated type `ty` has by its trait's declaration, if
    /// it is one, as [`crate::traits::alias_bounds`].
    fn alias_bounds(&mut self, ty: Ty) -> Vec<Predicate> {
        let ty = self.known(ty);
        crate::traits::alias_bounds(self.sigs, &self.param_env, ty)
    }

    fn select_predicate(&mut self, pred: Predicate) -> Selection {
        let self_ty = self.shallow(pred.self_ty);
        if self_ty == Ty::ERROR || pred.trait_ref.args.contains(&Ty::ERROR) {
            return Selection::Met(Vec::new());
        }
        if let TyKind::Infer(var) = self_ty.kind()
            && !self.unknown(var).0
        {
            return Selection::Ambiguous;
        }
        let t = pred.trait_ref.trait_;
        // Every type flux has is sized, except perhaps a trait's `Self`,
        // which is only if a bound says so.
        if self.items.traits[t].lang == Some(flux_intern::sym::sized)
            && self_ty != Ty::param(0, flux_intern::kw::SelfUpper)
        {
            return Selection::Met(Vec::new());
        }
        if crate::traits::is_builtin_copy(self.items, t, self_ty) {
            return Selection::Met(Vec::new());
        }
        // An array is compared with an array of its length.
        if let TyKind::Array(_, len) = self_ty.kind()
            && self.items.traits[t]
                .lang
                .is_some_and(|l| matches!(l.as_str(), "eq" | "partial_ord"))
            && let Some(&rhs) = pred.trait_ref.args.first()
            && let TyKind::Infer(var) = self.shallow(rhs).kind()
            && !self.unknown(var).0
        {
            let elem = self.new_var(false, TextRange::default());
            self.unify(rhs, Ty::array(elem, len));
        }
        let args: Vec<Ty> = pred
            .trait_ref
            .args
            .iter()
            .map(|&a| self.shallow(a))
            .collect();
        let goal = Predicate {
            self_ty,
            trait_ref: TraitRef {
                args: Ty::list(&args),
                ..pred.trait_ref
            },
        };
        let resolved = Predicate {
            self_ty: self.shallow_ref(goal.self_ty),
            trait_ref: TraitRef {
                args: Ty::list(
                    &goal
                        .trait_ref
                        .args
                        .iter()
                        .map(|&a| self.shallow_ref(a))
                        .collect::<Vec<_>>(),
                ),
                ..goal.trait_ref
            },
        };
        if let Some(elem) = crate::traits::builtin_array_impl(self.items, resolved) {
            return Selection::Met(vec![elem]);
        }
        if crate::traits::array_into_iter(self.items, t, self.shallow_ref(self_ty)).is_some() {
            return Selection::Met(Vec::new());
        }
        if let TyKind::Closure(..) = self_ty.kind() {
            return self.select_closure(pred);
        }
        if let Some(found) = crate::traits::fn_goal(self.items, t, self_ty) {
            let params = pred.trait_ref.args.first().copied().unwrap_or(Ty::ERROR);
            return match found {
                crate::traits::ClosureGoal::Fn(_, expected) if self.unify(params, expected) => {
                    Selection::Met(Vec::new())
                }
                _ => Selection::Unmet,
            };
        }
        let mut env: Vec<Predicate> = self
            .param_env
            .iter()
            .filter(|p| p.trait_ref.trait_ == t)
            .copied()
            .collect();
        let alias_bounds = self.alias_bounds(self_ty);
        env.extend(alias_bounds.into_iter().filter(|p| p.trait_ref.trait_ == t));
        let bounds: Vec<Predicate> = env
            .into_iter()
            .filter(|&p| self.probe(|cx| cx.unify_predicates(p, pred)))
            .collect();
        match bounds.as_slice() {
            [bound] => {
                self.unify_predicates(*bound, pred);
                return Selection::Met(Vec::new());
            }
            [] => {}
            _ => return Selection::Ambiguous,
        }
        let (items, sigs) = (self.items, self.sigs);
        let impls = sigs.impl_index.impls_for(items, t, self_ty);
        let found: Vec<ImplId> = impls
            .filter(|&imp| self.probe(|cx| cx.unify_impl(imp, pred).is_some()))
            .collect();
        let found = if found.len() > 1 {
            self.winnow(found, pred)
        } else {
            found
        };
        match found.as_slice() {
            [] => Selection::Unmet,
            &[imp] => {
                let args = self.unify_impl(imp, pred).expect("probed");
                self.register_impl_projections(imp, &args);
                let nested = self.sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
                Selection::Met(nested.iter().map(|p| p.subst(&args)).collect())
            }
            _ => Selection::Ambiguous,
        }
    }

    /// Of several blocks that may be for `pred`, those whose bounds may
    /// hold: the others are no candidates, as rustc's winnowing has it.
    fn winnow(&mut self, found: Vec<ImplId>, pred: Predicate) -> Vec<ImplId> {
        found
            .into_iter()
            .filter(|&imp| {
                self.probe(|cx| {
                    let Some(args) = cx.unify_impl(imp, pred) else {
                        return false;
                    };
                    let nested = cx.sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
                    nested.iter().all(|p| {
                        let p = cx.normalize_predicate(p.subst(&args), TextRange::default());
                        let args: Vec<Ty> = p.trait_ref.args.to_vec();
                        cx.may_hold(p.self_ty, p.trait_ref.trait_, Some(&args), 1)
                    })
                })
            })
            .collect()
    }

    fn unify_predicates(&mut self, a: Predicate, b: Predicate) -> bool {
        if a.trait_ref.trait_ != b.trait_ref.trait_ {
            return false;
        }
        let mut ok = self.unify(a.self_ty, b.self_ty);
        for (&x, &y) in a.trait_ref.args.iter().zip(b.trait_ref.args) {
            ok &= self.unify(x, y);
        }
        ok
    }

    /// Unifies `impl` block `imp`'s header, with new variables for its
    /// parameters, with `pred`: those variables, if it unifies.
    fn unify_impl(&mut self, imp: ImplId, pred: Predicate) -> Option<Vec<Ty>> {
        let (Some(&self_ty), Some(&trait_ref)) =
            (self.sigs.impls.get(imp), self.sigs.impl_traits.get(imp))
        else {
            return None;
        };
        let count = self.items.impls[imp].generics.len();
        let vars: Vec<Ty> = (0..count)
            .map(|_| self.new_var(false, TextRange::default()))
            .collect();
        let header = trait_ref.subst(&vars).with_self(self_ty.subst(&vars));
        self.unify_predicates(header, pred).then_some(vars)
    }

    /// Whether `ty` may implement `t`: through a bound in scope, or an
    /// `impl` block of `t` that may be for it and whose bounds may hold.
    pub(super) fn may_implement(&mut self, ty: Ty, t: TraitId) -> bool {
        self.probe(|cx| cx.may_hold(ty, t, None, 0))
    }

    /// Whether an `impl` block of `t` may be for `ty`, its bounds aside:
    /// a method found through it "exists" for rustc's messages.
    fn has_impl_header(&mut self, ty: Ty, t: TraitId) -> bool {
        self.probe(|cx| cx.may_hold(ty, t, None, u32::MAX - 1))
    }

    /// Whether a trait item called `name` exists for `ty` through an
    /// `impl` block whose bounds do not hold: rustc's "the method `m`
    /// exists for struct `W<S>`, but its trait bounds were not
    /// satisfied".
    pub(super) fn exists_unmet(&mut self, ty: Ty, name: Symbol, methods: bool) -> bool {
        let items = self.items;
        if !items.trait_fns.contains_key(&name) {
            return false;
        }
        let in_scope = self.traits_in_scope_with(name);
        in_scope.into_iter().any(|t| {
            let has = match items.traits[t].value(items, name) {
                Some(AssocItem::Function(f)) => !methods || items.functions[f].has_self,
                Some(AssocItem::Const(_)) => !methods,
                None => false,
            };
            has && self.has_impl_header(ty, t)
        })
    }

    /// Whether a method called `name` that takes `self` by value exists
    /// for a reference to `ty` through an `impl` block whose bounds do
    /// not hold, as rustc's probe finds `<&A as Ord>::max`, which needs
    /// `A: Ord`, for `a.max(b)` once it borrows `a`.
    pub(super) fn exists_unmet_for_ref(&mut self, ty: Ty, name: Symbol) -> bool {
        let items = self.items;
        if !items.trait_fns.contains_key(&name) {
            return false;
        }
        let in_scope = self.traits_in_scope_with(name);
        in_scope.into_iter().any(|t| {
            let by_value = match items.traits[t].value(items, name) {
                Some(AssocItem::Function(f)) => {
                    items.functions[f].has_self && super::self_mode(items, f).is_none()
                }
                _ => false,
            };
            by_value
                && [Mutability::Not, Mutability::Mut]
                    .into_iter()
                    .any(|m| self.has_impl_header(Ty::new(TyKind::Ref(m, ty)), t))
        })
    }

    /// Whether `ty: t` (with `args`, if given) may hold, unifying as it
    /// goes; call it in a probe.
    pub(super) fn may_hold(&mut self, ty: Ty, t: TraitId, args: Option<&[Ty]>, depth: u32) -> bool {
        let ty = self.shallow(ty);
        if ty == Ty::ERROR || matches!(ty.kind(), TyKind::Infer(v) if !self.unknown(v).0) {
            return true;
        }
        // Deep enough, or (from `has_impl_header`) bounds aside.
        let check_bounds = depth < 64;
        if self.items.traits[t].lang == Some(flux_intern::sym::sized)
            || crate::traits::is_builtin_copy(self.items, t, ty)
        {
            return true;
        }
        if crate::traits::array_into_iter(self.items, t, self.shallow_ref(ty)).is_some() {
            return true;
        }
        // An array compared with a slice, or a slice with an array.
        if self.items.traits[t].lang == Some(Symbol::intern("eq"))
            && let Some(&rhs) = args.and_then(|a| a.first())
        {
            let (lhs, rhs) = (self.shallow_ref(ty), self.shallow_ref(rhs));
            if !matches!(
                (lhs.kind(), rhs.kind()),
                (TyKind::Array(..), TyKind::Array(..))
            ) && let Some((a, b)) = crate::traits::array_eq_elems(lhs, rhs)
            {
                return self.may_hold(a, t, Some(&[b]), depth + 1);
            }
        }
        // `Default` for arrays of up to 32 elements, any for none.
        if let TyKind::Array(elem, len) = ty.kind()
            && self.items.traits[t]
                .lang
                .is_some_and(|l| l.as_str() == "Default")
        {
            return len == 0 || (len <= 32 && self.may_hold(elem, t, None, depth + 1));
        }
        if let TyKind::Array(elem, _) = ty.kind()
            && matches!(self.items.traits[t].lang, Some(l) if matches!(l.as_str(), "copy" | "clone" | "eq" | "total_eq" | "partial_ord" | "Ord" | "Debug"))
        {
            let args = args.map(|a| {
                a.iter()
                    .map(|&x| x.sequence_elem().unwrap_or(x))
                    .collect::<Vec<_>>()
            });
            return self.may_hold(elem, t, args.as_deref(), depth + 1);
        }
        if let Some(found) = crate::traits::fn_goal(self.items, t, ty) {
            return match found {
                crate::traits::ClosureGoal::Fn(_, params) => args
                    .and_then(|a| a.first())
                    .is_none_or(|&a| self.unify(a, params)),
                _ => false,
            };
        }
        if let TyKind::Closure(_, closure_args) = ty.kind() {
            return match crate::traits::closure_goal(self.items, t, closure_args) {
                crate::traits::ClosureGoal::Fn(kind, params) => {
                    let parts = crate::ClosureArgs::split(closure_args);
                    let kind_ok = crate::ClosureKind::from_ty(self.shallow(parts.kind))
                        .is_none_or(|k| k <= kind);
                    kind_ok
                        && args
                            .and_then(|a| a.first())
                            .is_none_or(|&a| self.unify(a, params))
                }
                crate::traits::ClosureGoal::Upvars(_) => true,
                crate::traits::ClosureGoal::No => false,
            };
        }
        let alias_bounds = self.alias_bounds(ty);
        let known = self.known(ty);
        let candidates: Vec<Predicate> = self
            .param_env
            .iter()
            .chain(&alias_bounds)
            .filter(|p| (p.self_ty == ty || p.self_ty == known) && p.trait_ref.trait_ == t)
            .copied()
            .collect();
        // A bound whose arguments unify with those wanted (`C: Index<usize>`
        // for `c[0]`, whose index is an integer variable).
        let in_env = candidates.iter().any(|p| {
            args.is_none_or(|a| {
                a.len() == p.trait_ref.args.len()
                    && self.probe(|cx| {
                        a.iter()
                            .zip(p.trait_ref.args)
                            .all(|(&x, &y)| cx.unify(x, y))
                    })
            })
        });
        if in_env {
            return true;
        }
        let (items, sigs) = (self.items, self.sigs);
        sigs.impl_index.impls_for(items, t, ty).any(|imp| {
            self.probe(|cx| {
                let (Some(&self_ty), Some(&trait_ref)) =
                    (cx.sigs.impls.get(imp), cx.sigs.impl_traits.get(imp))
                else {
                    return false;
                };
                let count = cx.items.impls[imp].generics.len();
                let vars: Vec<Ty> = (0..count)
                    .map(|_| cx.new_var(false, TextRange::default()))
                    .collect();
                if !cx.unify(self_ty.subst(&vars), ty) {
                    return false;
                }
                if let Some(args) = args {
                    let header = trait_ref.subst(&vars);
                    for (&x, &y) in header.args.iter().zip(args) {
                        if !cx.unify(x, y) {
                            return false;
                        }
                    }
                }
                if !check_bounds {
                    return true;
                }
                let nested = cx.sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
                nested.iter().all(|p| {
                    let p = cx.normalize_predicate(p.subst(&vars), TextRange::default());
                    let args: Vec<Ty> = p.trait_ref.args.to_vec();
                    cx.may_hold(p.self_ty, p.trait_ref.trait_, Some(&args), depth + 1)
                })
            })
        })
    }

    /// The traits whose methods may be called here: those named in the
    /// module's scope, and the prelude's.
    pub(super) fn traits_in_scope(&self) -> &[TraitId] {
        self.sigs.traits_in_scope.get(self.module)
    }

    /// The traits in scope with a function or constant called `name`,
    /// by index: those of the traits in scope or those with the name,
    /// whichever are fewer, that are also the others.
    fn traits_in_scope_with(&self, name: Symbol) -> Vec<TraitId> {
        let items = self.items;
        let in_scope = self.traits_in_scope();
        let with = items.trait_fns.get(&name).map_or(&[][..], |v| v);
        if with.len() < in_scope.len() {
            let mut found: Vec<TraitId> = with
                .iter()
                .copied()
                .filter(|t| {
                    in_scope
                        .binary_search_by_key(&t.index(), |u| u.index())
                        .is_ok()
                })
                .collect();
            found.sort_unstable_by_key(|t| t.index());
            found.dedup();
            found
        } else {
            in_scope
                .iter()
                .copied()
                .filter(|&t| items.traits[t].value(items, name).is_some())
                .collect()
        }
    }

    /// The methods called `name` of the traits in scope that `ty` may
    /// implement; with the trait's generic arguments when a bound gives
    /// them. `methods` for those with `self` only.
    pub(super) fn trait_items(
        &mut self,
        ty: Ty,
        name: Symbol,
        methods: bool,
    ) -> Vec<(FunctionId, Option<TraitRef>)> {
        self.trait_values(ty, name, methods)
            .into_iter()
            .filter_map(|(item, t)| match item {
                AssocItem::Function(f) => Some((f, t)),
                AssocItem::Const(_) => None,
            })
            .collect()
    }

    /// The functions and constants called `name` of the traits in scope
    /// that `ty` may implement, as [`Self::trait_items`]; functions only
    /// if `methods`, and those with `self`.
    fn trait_values(
        &mut self,
        ty: Ty,
        name: Symbol,
        methods: bool,
    ) -> Vec<(AssocItem, Option<TraitRef>)> {
        let ty = self.shallow(ty);
        let items = self.items;
        if !items.trait_fns.contains_key(&name) {
            return Vec::new();
        }
        let wanted = |t: TraitId| match items.traits[t].value(items, name)? {
            AssocItem::Function(f) if !methods || items.functions[f].has_self => {
                Some(AssocItem::Function(f))
            }
            AssocItem::Const(c) if !methods => Some(AssocItem::Const(c)),
            _ => None,
        };
        let mut found: Vec<(AssocItem, Option<TraitRef>)> = Vec::new();
        if ty.has_params() || matches!(ty.kind(), TyKind::Opaque(_)) {
            let alias_bounds = self.alias_bounds(ty);
            let known = self.known(ty);
            for p in self.param_env.clone().into_iter().chain(alias_bounds) {
                if (p.self_ty == ty || p.self_ty == known)
                    && let Some(f) = wanted(p.trait_ref.trait_)
                    && !found.iter().any(|(g, _)| *g == f)
                {
                    found.push((f, Some(p.trait_ref)));
                }
            }
        }
        if let TyKind::Infer(var) = ty.kind()
            && !self.unknown(var).0
        {
            return found;
        }
        let in_scope = self.traits_in_scope_with(name);
        for t in in_scope {
            let Some(f) = wanted(t) else {
                continue;
            };
            if found.iter().any(|(g, _)| *g == f) {
                continue;
            }
            if self.may_implement(ty, t) {
                found.push((f, None));
            }
        }
        found
    }

    /// The function or constant an associated path `callee` names: `T::f`,
    /// `S::f`, `<T as Tr>::f`, `<T>::f` or `Tr::f`, with the type it is
    /// on (`Self`, for a trait's) and the trait's generic arguments, if
    /// known. `None` if there is none (reported).
    pub(super) fn resolve_assoc(
        &mut self,
        callee: ExprId,
        ty: Option<flux_hir::TypeRefId>,
        trait_: Option<TraitId>,
        name: Symbol,
    ) -> Option<(AssocItem, Option<Ty>, Option<Vec<Ty>>)> {
        let segment = self.source_map.segments.get(callee).copied();
        let segment = segment.unwrap_or_else(|| self.range(callee));
        let self_ty = ty.map(|t| self.resolve_path_type(t));
        if self_ty == Some(Ty::ERROR) {
            return None;
        }
        if let Some(t) = trait_ {
            let items = self.items;
            let item = items.traits[t].value(items, name)?;
            let written = self
                .body
                .generic_args
                .get(callee)
                .and_then(|g| g.ty.clone());
            let trait_args = match written {
                Some(written) => {
                    let want = items.traits[t].generics.len();
                    let args: Vec<Ty> = written
                        .types
                        .iter()
                        .map(|&ty| self.resolve_type_ref(ty))
                        .collect();
                    if args.len() != want {
                        if !args.contains(&Ty::ERROR) {
                            let message = crate::wrong_arg_count("trait", want, args.len());
                            self.error(written.segment, message);
                        }
                        return None;
                    }
                    Some(args)
                }
                None => None,
            };
            return Some((item, self_ty, trait_args));
        }
        let self_ty = self.shallow(self_ty?);
        // `<S>::f`: the type's own items come before its traits'.
        if let Some(target) = crate::impl_target(self.items, self_ty) {
            let candidates = self.items.assoc_functions(target, name);
            if !candidates.is_empty() {
                return match self.select(candidates.into_iter(), Some(self_ty)) {
                    Ok(function) => Some((AssocItem::Function(function), Some(self_ty), None)),
                    Err(0) => {
                        let message = self.not_found(Some(self_ty), name);
                        self.error(segment, message);
                        None
                    }
                    Err(_) => {
                        self.error(segment, "multiple applicable items in scope");
                        None
                    }
                };
            }
            if let Some(AssocItem::Const(c)) = self.items.assoc_item(target, name) {
                return Some((AssocItem::Const(c), Some(self_ty), None));
            }
        }
        let found = self.trait_values(self_ty, name, false);
        match found.as_slice() {
            [(item, trait_ref)] => Some((*item, Some(self_ty), trait_ref.map(|t| t.args.to_vec()))),
            [] => {
                let items = self.items;
                let in_some_trait = items.trait_fns.contains_key(&name);
                if self.exists_unmet(self_ty, name, false) {
                    let message = format!(
                        "the associated function or constant `{name}` exists for {}, but its trait bounds were not satisfied",
                        self.describe_kind(self_ty)
                    );
                    self.error(segment, message);
                    return None;
                }
                let message = match self_ty.kind() {
                    TyKind::Int(_) | TyKind::Bool if !in_some_trait => {
                        format!("`{}::{name}` is not supported yet", self.describe(self_ty))
                    }
                    _ => self.not_found(Some(self_ty), name),
                };
                self.error(segment, message);
                None
            }
            _ => {
                self.error(segment, "multiple applicable items in scope");
                None
            }
        }
    }

    /// Registers the bounds a path to constant `konst` with generic
    /// arguments `args` needs: its trait's, for `Self`, or its block's;
    /// `target` is the type it was found on, if known.
    pub(super) fn register_const_bounds(
        &mut self,
        expr: ExprId,
        konst: flux_hir::ConstId,
        args: &[Ty],
        target: Option<Ty>,
    ) {
        let (items, sigs) = (self.items, self.sigs);
        let data = &items.consts[konst];
        let Some(name) = data.name else {
            return;
        };
        let segment = self.source_map.segments.get(expr).copied();
        let segment = segment.unwrap_or_else(|| self.range(expr));
        let known = target.filter(|&t| !self.has_unknown(t));
        let item_bounds = |ty| ObligationKind::ItemBounds {
            what: "associated function or constant",
            name,
            ty,
        };
        match data.owner {
            Some(Owner::Trait(t)) => {
                let pred = crate::traits::trait_self_predicate(items, t).subst(args);
                let qself = match self.body.exprs[expr] {
                    flux_hir::Expr::Path(flux_hir::Res::Assoc {
                        ty: Some(ty),
                        trait_: Some(_),
                        ..
                    }) => self.source_map.types.get(ty).copied(),
                    _ => None,
                };
                let (range, kind) = match (qself, known) {
                    (Some(qself), _) => (qself, ObligationKind::Bound),
                    (None, Some(ty)) => (segment, item_bounds(ty)),
                    (None, None) => (
                        segment,
                        ObligationKind::TraitCall {
                            range: self.range(expr),
                            constant: true,
                        },
                    ),
                };
                self.register(pred, range, kind);
                for p in sigs.trait_predicates.get(t).into_iter().flatten() {
                    self.register(p.subst(args), segment, ObligationKind::Bound);
                }
            }
            Some(Owner::Impl(imp)) => {
                for p in sigs.impl_predicates.get(imp).into_iter().flatten() {
                    let kind = match known {
                        Some(ty) => item_bounds(ty),
                        None => ObligationKind::Bound,
                    };
                    self.register(p.subst(args), segment, kind);
                }
            }
            None => {}
        }
        self.select_obligations();
    }

    /// In a trait's provided method, `Self` may be unsized unless a bound
    /// says otherwise, so no parameter or local may have it as its type.
    pub(super) fn check_unsized_self(&mut self) {
        let self_ty = Ty::param(0, flux_intern::kw::SelfUpper);
        let sized = self.param_env.iter().any(|p| {
            p.self_ty == self_ty
                && self.items.traits[p.trait_ref.trait_].lang == Some(flux_intern::sym::sized)
        });
        if sized {
            return;
        }
        let body = self.body;
        let message = "the size for values of type `Self` cannot be known at compilation time";
        // Calls that return `Self`, and the bindings that hold it.
        let mut ranges = Vec::new();
        for (expr, data) in body.exprs.iter() {
            if matches!(
                data,
                flux_hir::Expr::Call { .. } | flux_hir::Expr::MethodCall { .. }
            ) && let Some(&ty) = self.expr_types.get(expr)
                && self.shallow(ty) == self_ty
            {
                ranges.push(self.range(expr));
            }
        }
        for (pat, data) in body.pats.iter() {
            let flux_hir::Pat::Bind { binding, .. } = *data else {
                continue;
            };
            let ty = self
                .binding_types
                .get(binding)
                .copied()
                .unwrap_or(Ty::ERROR);
            if self.shallow(ty) == self_ty {
                ranges.push(self.source_map.pats.get(pat).copied().unwrap_or_default());
            }
        }
        for range in ranges {
            self.error(range, message);
        }
    }

    /// The type a path names before an associated item: as a type, except
    /// that a generic struct without arguments (`W::f`) gets new
    /// variables for them, as in an expression.
    fn resolve_path_type(&mut self, ty: flux_hir::TypeRefId) -> Ty {
        let items = self.items;
        if let flux_hir::TypeRef::Path(path) = &self.body.types[ty]
            && path.qself.is_none()
            && path.args(path.segments.len().saturating_sub(1)).is_none()
            && let Ok(flux_hir::def_map::Resolved::Def(flux_hir::def_map::Def::Struct(s))) =
                items.resolve(self.module, path, flux_hir::def_map::Ns::Types)
            && !items.structs[s].generics.is_empty()
        {
            let origin = self.source_map.types.get(ty).copied().unwrap_or_default();
            let count = items.structs[s].generics.len();
            let args: Vec<Ty> = (0..count).map(|_| self.new_var(false, origin)).collect();
            return Ty::adt(s, items.structs[s].name, &args);
        }
        self.resolve_type_ref(ty)
    }

    /// Whether a block for another instance of `ty`'s struct has a method
    /// `name`: rustc then names the struct with its parameters (`W<T>`)
    /// in "no method named" errors.
    pub(super) fn other_instance_has(&mut self, ty: Ty, name: Symbol) -> bool {
        let TyKind::Adt(adt, args) = ty.kind() else {
            return false;
        };
        let items = self.items;
        if args.is_empty() {
            return false;
        }
        let target = flux_hir::ImplTarget::Struct(adt.strukt);
        if items.assoc.contains_key(&(target, name)) {
            return true;
        }
        let traits = items.trait_fns.get(&name).map_or(&[][..], |v| v);
        let head = Some(crate::impl_index::Head::Adt(adt.strukt));
        traits.iter().any(|&t| {
            let mut impls = self.sigs.impl_index.impls_with_head(items, t, head);
            impls.any(|imp| {
                matches!(self.sigs.impls.get(imp).map(|t| t.kind()), Some(TyKind::Adt(a, _)) if a.strukt == adt.strukt)
            })
        })
    }

    /// Registers the bounds of a call of `function` with generic
    /// arguments `args`, at `site`; `target`, the type the item was found
    /// on if it was known then, makes its block's bounds rustc's "exists
    /// but its trait bounds were not satisfied".
    pub(super) fn register_call_bounds(
        &mut self,
        function: FunctionId,
        args: &[Ty],
        site: CallSite<'_>,
        target: Option<Ty>,
    ) {
        let (items, sigs) = (self.items, self.sigs);
        let data = &items.functions[function];
        let own = sigs.fn_predicates.get(function).map_or(0, |p| p.len());
        let owner_bounds = match data.owner {
            Some(Owner::Impl(imp)) => sigs.impl_predicates.get(imp).is_some_and(|p| !p.is_empty()),
            Some(Owner::Trait(_)) => true,
            None => false,
        };
        let projections = sigs
            .fn_projections
            .get(function)
            .is_some_and(|p| !p.is_empty())
            || match data.owner {
                Some(Owner::Impl(imp)) => sigs.impl_projections.get(imp).is_some(),
                Some(Owner::Trait(t)) => sigs.trait_projections.get(t).is_some(),
                None => false,
            };
        if own == 0 && !owner_bounds && !projections {
            return;
        }
        let preds = predicates_of(items, sigs, function);
        let owner_count = match data.owner {
            Some(Owner::Impl(imp)) => sigs.impl_predicates.get(imp).map_or(0, |p| p.len()),
            Some(Owner::Trait(t)) => 1 + sigs.trait_predicates.get(t).map_or(0, |p| p.len()),
            None => 0,
        };
        let what = if site.receiver.is_some() {
            "method"
        } else {
            "associated function or constant"
        };
        for (k, p) in preds.iter().enumerate() {
            let pred = p.subst(args);
            let known = target.filter(|&t| !self.has_unknown(t));
            let (range, kind) = match (data.owner, known) {
                (Some(Owner::Trait(_)), _) if k == 0 => match (site.qself, known) {
                    (Some(qself), _) => (qself, ObligationKind::Bound),
                    (None, Some(ty)) => (
                        site.callee,
                        ObligationKind::ItemBounds {
                            what,
                            name: data.name,
                            ty,
                        },
                    ),
                    (None, None) if site.receiver.is_some() => (site.callee, ObligationKind::Bound),
                    (None, None) => (
                        self.bound_range(function, *p, site),
                        ObligationKind::TraitCall {
                            range: site.whole,
                            constant: false,
                        },
                    ),
                },
                (Some(Owner::Impl(_)), Some(ty)) if k < owner_count => (
                    site.callee,
                    ObligationKind::ItemBounds {
                        what,
                        name: data.name,
                        ty,
                    },
                ),
                _ => (self.bound_range(function, *p, site), ObligationKind::Bound),
            };
            self.register(pred, range, kind);
        }
        // `T: Tr<Out = u8>`: what the arguments' associated types must be.
        let projections = crate::traits::declared_projections(items, sigs, function);
        for p in projections {
            // On a method's `Self` (`Self: Iterator<Item = &'a T>`), at
            // the receiver, as rustc reports it.
            let on_self = first_param(p.alias.args[0]) == Some(0)
                && matches!(items.functions[function].owner, Some(Owner::Trait(_)));
            if on_self && let Some(receiver) = site.receiver {
                let range = self.range(receiver);
                self.register_projection(p.subst(args), range);
                continue;
            }
            let range = self.bound_range(
                function,
                Predicate {
                    self_ty: p.alias.args[0],
                    trait_ref: TraitRef {
                        trait_: p.alias.trait_,
                        name: p.alias.trait_name,
                        args: Ty::list(&p.alias.args[1..]),
                    },
                },
                site,
            );
            self.register_projection(p.subst(args), range);
        }
        self.select_obligations();
    }

    /// Whether `ty` has variables nothing gave a type yet.
    pub(super) fn has_unknown(&mut self, ty: Ty) -> bool {
        let mut unknown = Vec::new();
        self.unknown_vars(ty, &mut unknown);
        self.int_vars(ty, &mut unknown);
        !unknown.is_empty()
    }

    /// Where rustc reports bound `p` of a call of `function` unmet: at
    /// the generic argument written for the parameter it bounds, else at
    /// the one argument whose type mentions it, else at the callee if
    /// several do, else at the whole call.
    fn bound_range(&self, function: FunctionId, p: Predicate, site: CallSite<'_>) -> TextRange {
        let (items, sigs) = (self.items, self.sigs);
        let data = &items.functions[function];
        let Some(param) = first_param(p.self_ty) else {
            return site.callee;
        };
        let first_own = owner_param_count(items, data.owner) as u32;
        if param < first_own {
            // A bound on the block's or trait's parameter in the
            // method's `where`.
            let own = sigs.fn_predicates.get(function).map_or(&[][..], |p| p);
            if own.contains(&p) {
                return site.callee;
            }
        }
        if param >= first_own
            && let Some(written) = self
                .body
                .generic_args
                .get(site.expr)
                .and_then(|g| g.item.as_ref())
            && let Some(&ty) = written.types.get((param - first_own) as usize)
            && let Some(&range) = self.source_map.types.get(ty)
        {
            return range;
        }
        let sig = &sigs[function];
        let mentions: Vec<usize> = sig
            .params
            .iter()
            .enumerate()
            .filter(|(_, ty)| mentions(**ty, param))
            .map(|(i, _)| i)
            .collect();
        match mentions.as_slice() {
            // A method call's is at the method's name, as rustc has it.
            [] if site.receiver.is_some() => site.callee,
            [] => site.whole,
            &[i] => {
                let expr = match site.receiver {
                    Some(receiver) if i == 0 => Some(receiver),
                    Some(_) => site.args.get(i - 1).copied(),
                    None => site.args.get(i).copied(),
                };
                expr.map_or(site.callee, |e| self.range(e))
            }
            _ => site.callee,
        }
    }
}

/// The first generic parameter `ty` mentions.
pub(super) fn first_param(ty: Ty) -> Option<u32> {
    match ty.kind() {
        TyKind::Param(p) => Some(p.index),
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
            first_param(t)
        }
        TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => {
            list.iter().find_map(|&t| first_param(t))
        }
        TyKind::FnDef(_) | TyKind::FnPtr(_) => ty.children().into_iter().find_map(first_param),
        _ => None,
    }
}

fn mentions(ty: Ty, index: u32) -> bool {
    crate::mentions_param(ty, index)
}
