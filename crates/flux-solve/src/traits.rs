//! Whether predicates hold outside of bodies: the bounds of items and
//! the supertraits they imply, whether a type implements a trait given
//! the bounds in scope, what associated types normalize to, and which
//! function a call of a trait's method calls once its types are known.
//!
//! Inside a body, where types have variables, inference selects `impl`
//! blocks itself (`flux_ty`'s `infer`); here every type is known, up to
//! the generic parameters in scope, which are treated as distinct types
//! that implement what their bounds say.

use flux_hir::{FunctionId, ImplId, ItemTree, Owner, StructId, TraitId};
use flux_intern::{Symbol, kw};

use crate::ty::{AliasTy, ClosureArgs, GenericArgs, Mutability, OpaqueTy, Ty, TyKind};
use crate::{ConstInstance, Instance, Signatures};
use flux_ty_ir::traits::{Bounds, Predicate, Projection, TraitRef};

/// `<self_ty as trait_ref>::name`.
pub fn alias_of(items: &ItemTree, trait_ref: TraitRef, self_ty: Ty, name: Symbol) -> AliasTy {
    let args: Vec<Ty> = std::iter::once(self_ty)
        .chain(trait_ref.args.iter().copied())
        .collect();
    AliasTy {
        trait_: trait_ref.trait_,
        trait_name: trait_ref.name,
        name,
        args: Ty::list(&args),
        owner_lifetimes: items.traits[trait_ref.trait_].generics.lifetimes.len() as u32,
        family_lifetimes: items.traits[trait_ref.trait_]
            .types
            .iter()
            .find(|ty| ty.name == name)
            .map_or(0, |ty| ty.generics.lifetimes.len() as u32),
        family_args: &[],
    }
}

/// How many generic parameters come before an associated item's own:
/// its `impl` block's, or its trait's and `Self`.
pub fn owner_param_count(items: &ItemTree, owner: Option<Owner>) -> usize {
    match owner {
        Some(Owner::Impl(imp)) => items.impls[imp].generics.len(),
        Some(Owner::Trait(t)) => 1 + items.traits[t].generics.len(),
        None => 0,
    }
}

/// `Self: Tr<P..>` for trait `t`, in terms of its own parameters.
pub fn trait_self_predicate(items: &ItemTree, t: TraitId) -> Predicate {
    let data = &items.traits[t];
    let args = crate::params_of(&data.generics, 1);
    Predicate {
        self_ty: Ty::param(0, kw::SelfUpper),
        trait_ref: TraitRef {
            trait_: t,
            name: data.name,
            args: Ty::list(&args),
        },
    }
}

/// The bounds of an associated item's owner: an `impl` block's, or a
/// trait's (`Self: Tr` first).
pub fn owner_predicates(
    items: &ItemTree,
    sigs: &Signatures,
    owner: Option<Owner>,
) -> Vec<Predicate> {
    let mut out = Vec::new();
    match owner {
        Some(Owner::Impl(imp)) => {
            out.extend(sigs.impl_predicates.get(imp).into_iter().flatten().copied());
        }
        Some(Owner::Trait(t)) => {
            out.push(trait_self_predicate(items, t));
            out.extend(sigs.trait_predicates.get(t).into_iter().flatten().copied());
        }
        None => {}
    }
    out
}

/// The bounds a function's body may rely on: its owner's (`Self: Tr` in
/// a trait), then its own, in terms of all its parameters, not yet
/// elaborated with supertraits.
pub fn predicates_of(items: &ItemTree, sigs: &Signatures, function: FunctionId) -> Vec<Predicate> {
    let mut out = owner_predicates(items, sigs, items.functions[function].owner);
    out.extend(
        sigs.fn_predicates
            .get(function)
            .into_iter()
            .flatten()
            .copied(),
    );
    out
}

/// What the bounds in scope in `function` say associated types are: its
/// owner's, then its own, then what their traits imply.
pub fn projections_of(
    items: &ItemTree,
    sigs: &Signatures,
    function: FunctionId,
) -> Vec<Projection> {
    let mut out = declared_projections(items, sigs, function);
    out.extend(implied_projections(
        sigs,
        &elaborate(sigs, &predicates_of(items, sigs, function)),
    ));
    out
}

/// Equalities belonging to caller proofs, rather than associated-type
/// declarations. Preserve their provenance before substitution: a declaration
/// Self can itself be an alias at a use site.
pub fn caller_projections_of(
    items: &ItemTree,
    sigs: &Signatures,
    function: FunctionId,
) -> Vec<Projection> {
    let mut out = caller_owner_projections(sigs, items.functions[function].owner);
    out.extend(
        sigs.fn_projections
            .get(function)
            .into_iter()
            .flatten()
            .copied(),
    );
    out.extend(caller_supertrait_projections(
        sigs,
        &elaborate(sigs, &predicates_of(items, sigs, function)),
    ));
    out
}

/// Caller-only equalities of an associated item's owner.
pub fn caller_owner_projections_of(
    items: &ItemTree,
    sigs: &Signatures,
    owner: Option<Owner>,
) -> Vec<Projection> {
    let mut out = caller_owner_projections(sigs, owner);
    out.extend(caller_supertrait_projections(
        sigs,
        &elaborate(sigs, &owner_predicates(items, sigs, owner)),
    ));
    out
}

/// Written owner bounds, excluding associated-type declaration equalities.
pub fn caller_owner_projections(sigs: &Signatures, owner: Option<Owner>) -> Vec<Projection> {
    let mut out = Vec::new();
    match owner {
        Some(Owner::Impl(imp)) => {
            out.extend(
                sigs.impl_projections
                    .get(imp)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        Some(Owner::Trait(t)) => {
            if let Some(sig) = sigs.traits.get(t) {
                out.extend(sig.header_projections.iter().copied());
            }
        }
        None => {}
    }
    out
}

/// Equalities on a bound's own Self, distinguished before substitution
/// from equalities originating in its associated-type declarations.
pub fn caller_supertrait_projections(sigs: &Signatures, env: &[Predicate]) -> Vec<Projection> {
    let mut out = Vec::new();
    let self_param = Ty::param(0, flux_intern::kw::SelfUpper);
    for p in env {
        let Some(sig) = sigs.traits.get(p.trait_ref.trait_) else {
            continue;
        };
        let args: Vec<Ty> = std::iter::once(p.self_ty)
            .chain(p.trait_ref.args.iter().copied())
            .collect();
        out.extend(
            sig.implied
                .iter()
                .filter(|q| q.alias.args[0] == self_param)
                .map(|q| q.subst(&args)),
        );
    }
    out
}

/// Independent implementation evidence for a moved supertrait equality. A
/// separate caller membership on the canonical receiver shadows impl evidence,
/// even when an impl could apply. Equalities for the key being justified are
/// excluded from requirement proofs and RHS normalization to avoid circularity.
pub fn signature_impl_evidence(
    items: &ItemTree,
    sigs: &Signatures,
    env: &[Predicate],
    unsized_params: &[bool],
    caller: &[Projection],
    required: Projection,
) -> bool {
    let alias = required.alias;
    let normalize_key = |a: AliasTy| a.map_args(|ty| normalize(items, sigs, caller, ty));
    let goal = alias_predicate(alias);
    if env.iter().any(|p| {
        p.self_ty == goal.self_ty
            && p.trait_ref.trait_ == goal.trait_ref.trait_
            && crate::ty::map_list(p.trait_ref.args, |ty| normalize(items, sigs, caller, ty))
                == goal.trait_ref.args
    }) {
        return false;
    }
    let independent: Vec<_> = caller
        .iter()
        .copied()
        .filter(|p| normalize_key(p.alias) != alias)
        .collect();
    let proof = SignatureImplProof {
        items,
        sigs,
        env: env
            .iter()
            .map(|p| SignatureImplProof::normalize(items, sigs, &independent, *p))
            .collect(),
        memberships: env,
        projections: independent,
        unsized_params,
        depth: std::cell::Cell::new(0),
    };
    let Some((imp, args)) = proof.select(goal, &mut Vec::new()) else {
        return false;
    };
    let Some(ty) = instantiate_impl_type(sigs, imp, alias, &args) else {
        return false;
    };
    let provided = Projection { alias, ty };
    proof.normalize_rhs(provided) == proof.normalize_rhs(required)
}

/// Unlike ordinary normalization's possible candidates, signature evidence
/// needs every substituted membership and associated equality to be proven.
pub struct SignatureImplProof<'a> {
    pub items: &'a ItemTree,
    pub sigs: &'a Signatures,
    // Normalized memberships keep associated impl RHSs abstract. They cannot
    // also prove requirements rewritten through caller equalities.
    pub env: Vec<Predicate>,
    // Keep caller predicates before equality normalization, including through
    // supertrait elaboration. A rewritten predicate is not independent evidence.
    pub memberships: &'a [Predicate],
    pub projections: Vec<Projection>,
    pub unsized_params: &'a [bool],
    pub depth: std::cell::Cell<u32>,
}

// Structural auto-trait proofs can cross several library containers per field.
// Keep normalization re-entry and predicate recursion under the same budget.
const SIGNATURE_RECURSION_LIMIT: usize = 128;

impl SignatureImplProof<'_> {
    fn caller_normalize(
        items: &ItemTree,
        sigs: &Signatures,
        projections: &[Projection],
        ty: Ty,
        guard: Option<AliasTy>,
    ) -> Ty {
        let mut normalizer = Normalizer {
            items,
            sigs,
            env: projections,
            caller_only: true,
            impl_proof: None,
            done: rustc_hash::FxHashMap::default(),
            active: guard.map(Ty::alias).into_iter().collect(),
        };
        normalizer.go(ty, 0)
    }

    pub fn normalize_ty(&self, ty: Ty, guard: Option<AliasTy>) -> Ty {
        let mut normalizer = Normalizer {
            items: self.items,
            sigs: self.sigs,
            env: &self.projections,
            caller_only: true,
            impl_proof: Some(self),
            done: rustc_hash::FxHashMap::default(),
            active: guard.map(Ty::alias).into_iter().collect(),
        };
        normalizer.go(ty, 0)
    }

    pub fn normalize_rhs(&self, p: Projection) -> Ty {
        self.normalize_ty(p.ty, Some(p.alias))
    }

    pub fn normalize(
        items: &ItemTree,
        sigs: &Signatures,
        projections: &[Projection],
        p: Predicate,
    ) -> Predicate {
        Predicate {
            self_ty: Self::caller_normalize(items, sigs, projections, p.self_ty, None),
            trait_ref: TraitRef {
                args: crate::ty::map_list(p.trait_ref.args, |ty| {
                    Self::caller_normalize(items, sigs, projections, ty, None)
                }),
                ..p.trait_ref
            },
        }
    }

    pub fn holds(&self, goal: Predicate, stack: &mut Vec<Predicate>) -> bool {
        if in_env(self.sigs, self.memberships, goal)
            || is_builtin_trait(self.items, goal.trait_ref.trait_, goal.self_ty)
            || (self.items.traits[goal.trait_ref.trait_].lang == Some(flux_intern::sym::sized)
                && !crate::maybe_unsized(
                    self.items,
                    self.sigs,
                    goal.self_ty,
                    self.unsized_params,
                    self.memberships,
                ))
            || matches!(fn_goal(self.items, goal.trait_ref.trait_, goal.self_ty),
                Some(ClosureGoal::Fn(_, params)) if goal.trait_ref.args.first() == Some(&params))
        {
            return true;
        }
        if let Some((_, coinductive)) = auto_cycle(self.items, goal, stack) {
            return coinductive;
        }
        if stack.len() >= SIGNATURE_RECURSION_LIMIT {
            return false;
        }
        if let Some(owner) = declared_alias_bound(self.sigs, goal) {
            stack.push(goal);
            let proven = self.holds(owner, stack);
            stack.pop();
            if proven {
                return true;
            }
        }
        if self.select(goal, stack).is_some() {
            return true;
        }
        let Some(nested) = auto_trait_requirements(self.items, self.sigs, goal) else {
            return false;
        };
        stack.push(goal);
        let proven = nested.into_iter().all(|p| self.holds(p, stack));
        stack.pop();
        proven
    }

    fn select(&self, goal: Predicate, stack: &mut Vec<Predicate>) -> Option<(ImplId, Vec<Ty>)> {
        // Normalizing an associated requirement can re-enter selection with a
        // fresh goal stack. Bound that path as well as membership recursion.
        let depth = self.depth.get();
        if depth as usize >= SIGNATURE_RECURSION_LIMIT {
            return None;
        }
        self.depth.set(depth + 1);
        let result = self.select_at(goal, stack);
        self.depth.set(depth);
        result
    }

    fn select_at(&self, goal: Predicate, stack: &mut Vec<Predicate>) -> Option<(ImplId, Vec<Ty>)> {
        if stack.len() >= SIGNATURE_RECURSION_LIMIT || stack.contains(&goal) {
            return None;
        }
        stack.push(goal);
        let mut found = None;
        for imp in self
            .sigs
            .impl_index
            .impls_for(self.items, goal.trait_ref.trait_, goal.self_ty)
        {
            let Some(args) = match_impl(self.items, self.sigs, imp, goal) else {
                continue;
            };
            if args.iter().any(|&ty| has_error(ty))
                || self.items.impls[imp]
                    .generics
                    .params
                    .iter()
                    .zip(&args)
                    .any(|(param, &ty)| {
                        !param.maybe_unsized
                            && crate::maybe_unsized(
                                self.items,
                                self.sigs,
                                ty,
                                self.unsized_params,
                                self.memberships,
                            )
                    })
            {
                continue;
            }
            let memberships = self.sigs.impl_predicates.get(imp).into_iter().flatten();
            if !memberships.copied().all(|p| {
                let p = p.subst(&args);
                let p = Predicate {
                    self_ty: self.normalize_ty(p.self_ty, None),
                    trait_ref: TraitRef {
                        args: crate::ty::map_list(p.trait_ref.args, |ty| {
                            self.normalize_ty(ty, None)
                        }),
                        ..p.trait_ref
                    },
                };
                !has_error(p.self_ty)
                    && !p.trait_ref.args.iter().any(|&ty| has_error(ty))
                    && self.holds(p, stack)
            }) {
                continue;
            }
            let equalities = self.sigs.impl_projections.get(imp).into_iter().flatten();
            if !equalities.copied().all(|p| {
                let p = p.subst(&args);
                let lhs = self.normalize_ty(Ty::alias(p.alias), None);
                let rhs = self.normalize_rhs(p);
                !has_error(lhs) && !has_error(rhs) && lhs == rhs
            }) {
                continue;
            }
            if found.is_some() {
                stack.pop();
                return None;
            }
            found = Some((imp, args));
        }
        stack.pop();
        found
    }
}

/// What the bounds of an associated item's owner say associated types
/// are: as written, then what their traits imply.
pub fn owner_projections(
    items: &ItemTree,
    sigs: &Signatures,
    owner: Option<Owner>,
) -> Vec<Projection> {
    let mut out: Vec<Projection> = match owner {
        Some(Owner::Impl(imp)) => sigs
            .impl_projections
            .get(imp)
            .into_iter()
            .flatten()
            .copied()
            .collect(),
        Some(Owner::Trait(t)) => sigs
            .trait_projections
            .get(t)
            .into_iter()
            .flatten()
            .copied()
            .collect(),
        None => Vec::new(),
    };
    out.extend(implied_projections(
        sigs,
        &elaborate(sigs, &owner_predicates(items, sigs, owner)),
    ));
    out
}

/// The associated types the bounds of `function` and its block bind,
/// as written: what a call must show.
pub fn declared_projections(
    items: &ItemTree,
    sigs: &Signatures,
    function: FunctionId,
) -> Vec<Projection> {
    let mut out = Vec::new();
    match items.functions[function].owner {
        Some(Owner::Impl(imp)) => {
            out.extend(
                sigs.impl_projections
                    .get(imp)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        Some(Owner::Trait(t)) => {
            out.extend(sigs.trait_projections.get(t).into_iter().flatten().copied());
        }
        None => {}
    }
    out.extend(
        sigs.fn_projections
            .get(function)
            .into_iter()
            .flatten()
            .copied(),
    );
    out
}

/// `predicates` with the supertraits each implies, transitively, without
/// repeats. The bounds of their associated types (`type Out: Bound;`) are
/// left to [`alias_bounds`], as rustc leaves them: `type Inner: Tr;`
/// would go on forever, and nested ones multiply.
pub fn elaborate(sigs: &Signatures, predicates: &[Predicate]) -> Vec<Predicate> {
    // Bounds without supertraits are common. Preserve first-occurrence order
    // without allocating a traversal stack or a set for a short input.
    if predicates.len() <= 8
        && predicates.iter().all(|p| {
            sigs.traits
                .get(p.trait_ref.trait_)
                .is_none_or(|sig| sig.supertraits.is_empty())
        })
    {
        let mut out = Vec::with_capacity(predicates.len());
        for &p in predicates {
            if !out.contains(&p) {
                out.push(p);
            }
        }
        return out;
    }
    let mut out: Vec<Predicate> = Vec::new();
    let mut seen = rustc_hash::FxHashSet::default();
    // Repeating a trait on the current path is an invalid supertrait cycle,
    // even when substitution keeps changing its arguments. Full-predicate
    // deduplication alone cannot stop `trait A<T>: A<(T, T)>`. Keep other
    // paths independent: a diamond can imply the same trait with different
    // arguments. The cycle checker reports the source diagnostic separately.
    let mut active = rustc_hash::FxHashSet::default();
    let mut stack: Vec<_> = predicates.iter().rev().map(|&p| (p, false)).collect();
    while let Some((p, exiting)) = stack.pop() {
        if exiting {
            active.remove(&p.trait_ref.trait_);
            continue;
        }
        if active.contains(&p.trait_ref.trait_) || !seen.insert(p) {
            continue;
        }
        out.push(p);
        let Some(sig) = sigs.traits.get(p.trait_ref.trait_) else {
            continue;
        };
        if sig.supertraits.is_empty() {
            continue;
        }
        active.insert(p.trait_ref.trait_);
        stack.push((p, true));
        let count = 1 + p.trait_ref.args.len();
        let mut short = [Ty::ERROR; 16];
        let mut long = Vec::new();
        let args = if count <= short.len() {
            short[0] = p.self_ty;
            short[1..count].copy_from_slice(p.trait_ref.args);
            &short[..count]
        } else {
            long.reserve(count);
            long.push(p.self_ty);
            long.extend_from_slice(p.trait_ref.args);
            &long
        };
        for sup in sig.supertraits.iter().rev() {
            stack.push((sup.subst(args).with_self(p.self_ty), false));
        }
    }
    out
}

/// Associated-name lookup in a trait impl may use its substituted
/// supertraits. These are lookup candidates, not declaration assumptions.
pub fn associated_self_bounds(
    sigs: &Signatures,
    trait_ref: Option<TraitRef>,
    self_ty: Option<Ty>,
) -> Vec<Predicate> {
    match (trait_ref, self_ty) {
        (Some(trait_ref), Some(self_ty)) => elaborate(sigs, &[trait_ref.with_self(self_ty)]),
        _ => Vec::new(),
    }
}

/// Concrete inherited definitions in the current impl's type namespace.
/// Their lifetime tables are rebased separately by region signature resolution.
pub fn associated_self_types(
    items: &ItemTree,
    sigs: &Signatures,
    candidates: &[Predicate],
) -> Vec<(TraitRef, Symbol, Ty)> {
    let mut out = Vec::new();
    for &goal in candidates.iter().skip(1) {
        let Some((imp, args)) = select_impl(items, sigs, goal) else {
            continue;
        };
        for associated in &items.traits[goal.trait_ref.trait_].types {
            if !associated.generics.params.is_empty() || !associated.generics.lifetimes.is_empty() {
                continue;
            }
            if let Some(ty) = impl_type(sigs, imp, associated.name) {
                out.push((goal.trait_ref, associated.name, ty.subst(&args)));
            }
        }
    }
    out
}

/// The bounds associated type `ty` has by its trait, elaborated, where
/// `env` says its `Self` implements the trait: `<T as Tr>::Out: Bound`
/// from `type Out: Bound;` in `Tr`, given `T: Tr`. None for other types.
pub fn alias_bounds(sigs: &Signatures, env: &[Predicate], ty: Ty) -> Vec<Predicate> {
    if let TyKind::Opaque(opaque) = ty.kind() {
        return opaque_bounds(sigs, opaque);
    }
    if let TyKind::Dynamic(d) = ty.kind() {
        return dyn_bounds(sigs, ty, d);
    }
    let TyKind::Alias(alias) = ty.kind() else {
        return Vec::new();
    };
    let Some(sig) = sigs.traits.get(alias.trait_) else {
        return Vec::new();
    };
    if !sig.type_bounds.iter().any(|&(name, _)| name == alias.name)
        || !in_env(sigs, env, alias_predicate(alias))
    {
        return Vec::new();
    }
    let bounds: Vec<Predicate> = sig
        .type_bounds
        .iter()
        .filter(|&&(name, _)| name == alias.name)
        .map(|&(_, bound)| bound.subst(&alias.template_args()).with_self(ty))
        .collect();
    elaborate(sigs, &bounds)
}

/// Whether `env`, elaborated bounds, has `goal`, or the bounds of its
/// `Self`'s trait do if that is an associated type.
pub fn in_env(sigs: &Signatures, env: &[Predicate], goal: Predicate) -> bool {
    env.contains(&goal)
        || (matches!(
            goal.self_ty.kind(),
            TyKind::Alias(_) | TyKind::Opaque(_) | TyKind::Dynamic(_)
        ) && alias_bounds(sigs, env, goal.self_ty).contains(&goal))
}

// A declared output bound is usable only after independently proving that
// the projection's receiver implements its owning trait.
fn declared_alias_bound(sigs: &Signatures, goal: Predicate) -> Option<Predicate> {
    let TyKind::Alias(alias) = goal.self_ty.kind() else {
        return None;
    };
    let owner = alias_predicate(alias);
    alias_bounds(sigs, &[owner], goal.self_ty)
        .contains(&goal)
        .then_some(owner)
}

/// What trait object `ty`, `d`, implements: its trait, with its
/// supertraits.
pub fn dyn_bounds(sigs: &Signatures, ty: Ty, d: crate::DynTy) -> Vec<Predicate> {
    let mut bounds = elaborate(sigs, &[dyn_principal(ty, d)]);
    bounds.extend(d.auto_traits.iter().map(|&trait_ref| Predicate {
        self_ty: ty,
        trait_ref,
    }));
    bounds
}

/// `ty: Tr<A>` for trait object `ty`, `d`, `dyn Tr<A>`.
pub fn dyn_principal(ty: Ty, d: crate::DynTy) -> Predicate {
    Predicate {
        self_ty: ty,
        trait_ref: TraitRef {
            trait_: d.trait_,
            name: d.name,
            args: d.args,
        },
    }
}

/// What trait object `ty`, `d`, says the associated types of its
/// trait and supertraits are, and what their traits imply.
pub fn dyn_projections(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    d: crate::DynTy,
) -> Vec<Projection> {
    let bounds = dyn_bounds(sigs, ty, d);
    let mut out = Vec::new();
    for p in &bounds {
        for (&name, &assoc) in d.assoc_names.iter().zip(d.assoc_tys) {
            if !items.traits[p.trait_ref.trait_]
                .types
                .iter()
                .any(|ty| ty.name == name)
            {
                continue;
            }
            out.push(Projection {
                alias: alias_of(items, p.trait_ref, ty, name),
                ty: assoc,
            });
        }
    }
    out.extend(implied_projections(sigs, &bounds));
    out
}

/// The bounds of `opaque`, with their supertraits: all that is known of
/// it outside its function's body, as rustc's item bounds of an opaque
/// type are.
pub fn opaque_bounds(sigs: &Signatures, opaque: OpaqueTy) -> Vec<Predicate> {
    let Some(sig) = opaque_sig(sigs, opaque) else {
        return Vec::new();
    };
    let args = opaque_args(opaque);
    let bounds: Vec<Predicate> = sig.predicates.iter().map(|p| p.subst(&args)).collect();
    elaborate(sigs, &bounds)
}

/// What the bounds of `opaque` say its associated types are
/// (`impl Iterator<Item = u32>`), and what their traits say.
pub fn opaque_projections(sigs: &Signatures, opaque: OpaqueTy) -> Vec<Projection> {
    let Some(sig) = opaque_sig(sigs, opaque) else {
        return Vec::new();
    };
    let args = opaque_args(opaque);
    let mut out: Vec<Projection> = sig.projections.iter().map(|p| p.subst(&args)).collect();
    out.extend(implied_projections(sigs, &opaque_bounds(sigs, opaque)));
    out
}

fn opaque_sig(sigs: &Signatures, opaque: OpaqueTy) -> Option<&crate::OpaqueSig> {
    sigs.opaques
        .get(opaque.function)?
        .get(opaque.index as usize)
}

/// What an opaque type's bounds are substituted with: its arguments,
/// then itself.
pub fn opaque_args(opaque: OpaqueTy) -> Vec<Ty> {
    let mut args = opaque.args.to_vec();
    args.push(Ty::opaque(opaque));
    args
}

/// `ty` with each opaque type replaced by its hidden type, as far as
/// [`Signatures::hidden`] has them: what code generation sees.
pub fn reveal(sigs: &Signatures, ty: Ty) -> Ty {
    reveal_at(sigs, ty, 0)
}

/// `ty`, in `function`'s signature, with the function's own opaque
/// types replaced by their hidden types, and no others: the types its
/// body has.
pub fn reveal_own(sigs: &Signatures, function: FunctionId, ty: Ty) -> Ty {
    if !ty.has_opaques() {
        return ty;
    }
    ty.map_opaques(&mut |o| match sigs.hidden.get(&(o.function, o.index)) {
        Some(&hidden) if o.function == function => hidden.subst(o.args),
        _ => Ty::opaque(o),
    })
}

fn reveal_at(sigs: &Signatures, ty: Ty, depth: u32) -> Ty {
    if !ty.has_opaques() || depth > 64 {
        return ty;
    }
    ty.map_opaques(&mut |o| match sigs.hidden.get(&(o.function, o.index)) {
        Some(&hidden) => reveal_at(sigs, hidden.subst(o.args), depth + 1),
        None => Ty::opaque(o),
    })
}

/// `Self: Tr` for `<Self as Tr>::Out`.
pub fn alias_predicate(alias: AliasTy) -> Predicate {
    Predicate {
        self_ty: alias.args[0],
        trait_ref: TraitRef {
            trait_: alias.trait_,
            name: alias.trait_name,
            args: Ty::list(&alias.args[1..]),
        },
    }
}

/// Obligations for selecting this occurrence of an associated family.
/// These are requirements on callers, never new assumptions about its owner.
pub fn family_requirements(items: &ItemTree, sigs: &Signatures, alias: AliasTy) -> Bounds {
    let Some(family) = sigs
        .traits
        .get(alias.trait_)
        .and_then(|signature| {
            signature
                .families
                .iter()
                .find(|(name, _)| *name == alias.name)
        })
        .map(|(_, family)| family)
    else {
        return Bounds::default();
    };
    let args = alias.template_args();
    let mut requirements = Bounds {
        predicates: family
            .predicates
            .iter()
            .map(|predicate| predicate.subst(&args))
            .collect(),
        projections: family
            .projections
            .iter()
            .map(|projection| projection.subst(&args))
            .collect(),
    };
    if let Some(&sized) = items.lang_traits.get(&flux_intern::sym::sized)
        && let Some(declaration) = items.traits[alias.trait_]
            .types
            .iter()
            .find(|ty| ty.name == alias.name)
    {
        for (parameter, &argument) in declaration.generics.params.iter().zip(alias.family_args) {
            if !parameter.is_const() && !parameter.maybe_unsized {
                requirements.predicates.push(Predicate {
                    self_ty: argument,
                    trait_ref: TraitRef {
                        trait_: sized,
                        name: items.traits[sized].name,
                        args: &[],
                    },
                });
            }
        }
    }
    requirements
}

/// What associated types of associated type `alias` are by its trait's
/// declaration: `<T::N as Add>::Output == i32` for `T::N` from
/// `type N: Add<Output = i32>;`, and what its bounds' traits say.
pub fn alias_projections(sigs: &Signatures, alias: AliasTy) -> Vec<Projection> {
    let Some(sig) = sigs.traits.get(alias.trait_) else {
        return Vec::new();
    };
    let ty = Ty::alias(alias);
    let mut out: Vec<Projection> = sig
        .implied
        .iter()
        .map(|q| q.subst(&alias.template_args()))
        .filter(|q| q.alias.args[0] == ty)
        .collect();
    let bounds: Vec<Predicate> = sig
        .type_bounds
        .iter()
        .filter(|&&(name, _)| name == alias.name)
        .map(|&(_, bound)| bound.subst(&alias.template_args()).with_self(ty))
        .collect();
    out.extend(implied_projections(sigs, &elaborate(sigs, &bounds)));
    out
}

/// What the associated types in `env`, elaborated bounds, are by what
/// their traits say: `T::N as Add>::Output == i32` from `T: Num` with
/// `type N: Add<Output = i32>;` in `Num`.
pub fn implied_projections(sigs: &Signatures, env: &[Predicate]) -> Vec<Projection> {
    let mut out = Vec::new();
    let mut associated = Vec::new();
    for p in env {
        let Some(sig) = sigs.traits.get(p.trait_ref.trait_) else {
            continue;
        };
        if sig.implied.is_empty() {
            continue;
        }
        let args: Vec<Ty> = std::iter::once(p.self_ty)
            .chain(p.trait_ref.args.iter().copied())
            .collect();
        for q in sig
            .implied
            .iter()
            .filter(|q| {
                // Own family type/const parameters are not bound by membership
                // alone. Instantiate these equalities at alias_projections instead.
                crate::max_param(Ty::alias(q.alias)).max(crate::max_param(q.ty))
                    <= args.len() as u32
            })
            .map(|q| q.subst(&args))
        {
            // An explicit bound on an associated type, including its
            // supertraits, decides normalization before the type's
            // declaration bounds. For `T: H, T::Part: Q`, Q's
            // `Self: P<A = u8>` precedes H's `Part: P<A = u16>`.
            // Keep both equalities: predicate deduplication does not
            // discharge projection obligations at calls or impls.
            if q.alias.args[0] == p.self_ty {
                out.push(q);
            } else {
                associated.push(q);
            }
        }
    }
    out.extend(associated);
    out
}

/// Binds the parameters in `pattern` (an `impl` block's header, in terms
/// of its parameters) so that it is `ty`: whether it can be. Other
/// parameters in `ty` are rigid types.
pub fn match_ty(pattern: Ty, ty: Ty, binds: &mut [Option<Ty>]) -> bool {
    if pattern == ty && !pattern.has_params() {
        return true;
    }
    match (pattern.kind(), ty.kind()) {
        (TyKind::Param(p), _) => match binds.get(p.index as usize) {
            Some(Some(bound)) => *bound == ty,
            Some(None) => {
                binds[p.index as usize] = Some(ty);
                true
            }
            None => pattern == ty,
        },
        (TyKind::Error, _) | (_, TyKind::Error) => true,
        (TyKind::Adt(a, xs), TyKind::Adt(b, ys)) => {
            a == b && xs.iter().zip(ys).all(|(&x, &y)| match_ty(x, y, binds))
        }
        (TyKind::Dynamic(a), TyKind::Dynamic(b)) => {
            a.trait_ == b.trait_
                && a.auto_traits == b.auto_traits
                && a.assoc_names == b.assoc_names
                && a.args.len() == b.args.len()
                && a.assoc_tys.len() == b.assoc_tys.len()
                && a.args
                    .iter()
                    .chain(a.assoc_tys)
                    .zip(b.args.iter().chain(b.assoc_tys))
                    .all(|(&a, &b)| match_ty(a, b, binds))
        }
        (TyKind::Closure(a, xs), TyKind::Closure(b, ys)) => {
            a == b && xs.iter().zip(ys).all(|(&x, &y)| match_ty(x, y, binds))
        }
        (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(&x, &y)| match_ty(x, y, binds))
        }
        (TyKind::Ref(m, x), TyKind::Ref(n, y)) | (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y)) => {
            m == n && match_ty(x, y, binds)
        }
        (TyKind::Array(x, m), TyKind::Array(y, n)) => {
            match_ty(m, n, binds) && match_ty(x, y, binds)
        }
        (TyKind::Slice(x), TyKind::Slice(y)) => match_ty(x, y, binds),
        (TyKind::FnPtr(x), TyKind::FnPtr(y)) => {
            x.is_unsafe == y.is_unsafe
                && x.is_c == y.is_c
                && x.inputs_output.len() == y.inputs_output.len()
                && (x.inputs_output.iter().zip(y.inputs_output))
                    .all(|(&a, &b)| match_ty(a, b, binds))
        }
        (TyKind::Alias(a), TyKind::Alias(b)) => {
            a.trait_ == b.trait_
                && a.name == b.name
                && a.owner_lifetimes == b.owner_lifetimes
                && a.family_lifetimes == b.family_lifetimes
                && a.args.len() == b.args.len()
                && a.family_args.len() == b.family_args.len()
                && a.args
                    .iter()
                    .chain(a.family_args)
                    .zip(b.args.iter().chain(b.family_args))
                    .all(|(&x, &y)| match_ty(x, y, binds))
        }
        (TyKind::Opaque(a), TyKind::Opaque(b)) => {
            (a.function, a.index) == (b.function, b.index)
                && a.args
                    .iter()
                    .zip(b.args)
                    .all(|(&x, &y)| match_ty(x, y, binds))
        }
        _ => pattern == ty,
    }
}

/// What `impl` block `imp` says its associated type `name` is, in terms
/// of its parameters.
pub fn impl_type(sigs: &Signatures, imp: ImplId, name: Symbol) -> Option<Ty> {
    let types = sigs.impl_types.get(imp)?;
    types.iter().find(|(n, _)| *n == name).map(|&(_, ty)| ty)
}

/// Apply family arguments only after selecting and instantiating the impl header.
pub fn instantiate_impl_type(
    sigs: &Signatures,
    imp: ImplId,
    alias: AliasTy,
    impl_args: &[Ty],
) -> Option<Ty> {
    let ty = impl_type(sigs, imp, alias.name)?;
    if let Some(specification) = sigs
        .impl_families
        .get(imp)
        .and_then(|families| families.iter().find(|(name, _)| *name == alias.name))
        .map(|(_, signature)| signature)
    {
        if specification.owner_params as usize != impl_args.len()
            || specification.lifetimes != alias.family_lifetimes
            || specification.params.len() != alias.family_args.len()
        {
            return None;
        }
    } else if alias.family_lifetimes != 0 || !alias.family_args.is_empty() {
        return None;
    }
    let args: Vec<_> = impl_args.iter().chain(alias.family_args).copied().collect();
    Some(ty.subst(&args))
}

/// `ty` with each associated type replaced by what it is, where that is
/// known: from `env`, the bounds in scope, or from the `impl` block for
/// its types. The others stay, as types of their own.
pub fn normalize(items: &ItemTree, sigs: &Signatures, env: &[Projection], ty: Ty) -> Ty {
    if !ty.has_aliases() && !ty.has_opaques() {
        return ty;
    }
    let mut normalizer = Normalizer {
        items,
        sigs,
        env,
        caller_only: false,
        impl_proof: None,
        done: rustc_hash::FxHashMap::default(),
        active: Vec::new(),
    };
    normalizer.go(ty, 0)
}

/// Normalize a constraint's right side without proving it from the very
/// projection being constrained. A recursive reference to that projection
/// stays rigid instead of taking the first competing equality in `env`.
pub fn normalize_projection_rhs(
    items: &ItemTree,
    sigs: &Signatures,
    env: &[Projection],
    projection: Projection,
) -> Ty {
    let mut normalizer = Normalizer {
        items,
        sigs,
        env,
        caller_only: false,
        impl_proof: None,
        done: rustc_hash::FxHashMap::default(),
        active: vec![Ty::alias(projection.alias)],
    };
    normalizer.go(projection.ty, 0)
}

/// One [`normalize`]: what each type it met normalizes to, as aliases
/// recur in what they normalize to (`<T as Tr>::A` in an `impl` block's
/// `type B`), which would otherwise be normalized again each time.
struct Normalizer<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    env: &'a [Projection],
    /// Signature impl evidence must not normalize through possible impls.
    caller_only: bool,
    impl_proof: Option<&'a SignatureImplProof<'a>>,
    done: rustc_hash::FxHashMap<Ty, Ty>,
    /// The aliases being normalized: one met again within its own
    /// normalization is a cycle, and stays as it is.
    active: Vec<Ty>,
}

impl Normalizer<'_> {
    fn go(&mut self, ty: Ty, depth: u32) -> Ty {
        if (!ty.has_aliases() && !ty.has_opaques()) || depth > 64 {
            return ty;
        }
        if let Some(&done) = self.done.get(&ty) {
            return done;
        }
        if self.active.contains(&ty) {
            return ty;
        }
        self.active.push(ty);
        let normalized = self.normalize(ty, depth);
        self.active.pop();
        self.done.insert(ty, normalized);
        normalized
    }

    fn normalize(&mut self, ty: Ty, depth: u32) -> Ty {
        let (items, sigs) = (self.items, self.sigs);
        match ty.kind() {
            TyKind::Opaque(opaque) => {
                let args = crate::ty::map_list(opaque.args, |arg| self.go(arg, depth + 1));
                let opaque = OpaqueTy { args, ..opaque };
                if !self.caller_only
                    && matches!(
                        items.functions[opaque.function].owner,
                        Some(Owner::Trait(_))
                    )
                {
                    let target = resolve_instance(
                        items,
                        sigs,
                        Instance {
                            function: opaque.function,
                            args,
                        },
                    );
                    if target.function != opaque.function
                        && let Some(signature) = sigs.fns.get(target.function)
                        && opaque.index == 0
                        && matches!(sigs.fns[opaque.function].ret.kind(), TyKind::Opaque(_))
                    {
                        return self.go(signature.ret.subst(target.args), depth + 1);
                    }
                }
                Ty::opaque(opaque)
            }
            TyKind::Alias(alias) => {
                let alias = alias.map_args(|a| self.go(a, depth + 1));
                let args = alias.args;
                if let Some(p) = self.env.iter().find(|p| p.alias == alias) {
                    return self.go(p.ty, depth + 1);
                }
                // Input normalization can change an environment key too,
                // e.g. P<<W<T> as Select>::Out> and P<T>. Normalize only
                // its inputs, never the constrained projection itself.
                let env = self.env;
                for p in env {
                    if p.alias.trait_ == alias.trait_
                        && p.alias.name == alias.name
                        && p.ty != Ty::alias(p.alias)
                        && p.alias.map_args(|ty| self.go(ty, depth + 1)) == alias
                    {
                        return self.go(p.ty, depth + 1);
                    }
                }
                if let Some(ret) = closure_output(items, alias) {
                    return self.go(ret, depth + 1);
                }
                if self.caller_only {
                    if let Some(proof) = self.impl_proof
                        // A caller membership without an RHS remains abstract,
                        // even when a matching impl has proven requirements.
                        && !in_env(sigs, &proof.env, alias_predicate(alias))
                        && let Some((imp, args)) =
                            proof.select(alias_predicate(alias), &mut Vec::new())
                        && let Some(ty) = instantiate_impl_type(sigs, imp, alias, &args)
                    {
                        return self.go(ty, depth + 1);
                    }
                    return Ty::alias(alias);
                }
                if let TyKind::Alias(outer) = args[0].kind()
                    && let Some(p) = alias_projections(sigs, outer)
                        .into_iter()
                        .find(|p| p.alias == alias)
                {
                    return self.go(p.ty, depth + 1);
                }
                if let TyKind::Opaque(opaque) = args[0].kind()
                    && let Some(p) = opaque_projections(sigs, opaque)
                        .into_iter()
                        .find(|p| p.alias == alias)
                {
                    return self.go(p.ty, depth + 1);
                }
                if let TyKind::Dynamic(d) = args[0].kind()
                    && let Some(p) = dyn_projections(self.items, sigs, args[0], d)
                        .into_iter()
                        .find(|p| p.alias == alias)
                {
                    return self.go(p.ty, depth + 1);
                }
                let goal = alias_predicate(alias);
                if let Some((imp, impl_args)) = select_impl(items, sigs, goal)
                    && let Some(t) = instantiate_impl_type(sigs, imp, alias, &impl_args)
                {
                    return self.go(t, depth + 1);
                }
                Ty::alias(alias)
            }
            TyKind::Tuple(list) => {
                let list = crate::ty::map_list(list, |t| self.go(t, depth + 1));
                Ty::new(TyKind::Tuple(list))
            }
            TyKind::Adt(adt, list) => {
                let list = crate::ty::map_list(list, |t| self.go(t, depth + 1));
                Ty::new(TyKind::Adt(adt, list))
            }
            TyKind::Closure(id, list) => {
                let list = crate::ty::map_list(list, |t| self.go(t, depth + 1));
                Ty::new(TyKind::Closure(id, list))
            }
            TyKind::Ref(m, t) => Ty::new(TyKind::Ref(m, self.go(t, depth + 1))),
            TyKind::RawPtr(m, t) => Ty::raw_ptr(m, self.go(t, depth + 1)),
            TyKind::Array(t, len) => Ty::array_of(self.go(t, depth + 1), len),
            TyKind::Slice(t) => Ty::slice(self.go(t, depth + 1)),
            TyKind::FnDef(_) | TyKind::FnPtr(_) => ty.map_fn(|t| self.go(t, depth + 1)),
            TyKind::Dynamic(d) => Ty::new(TyKind::Dynamic(d.map(|t| self.go(t, depth + 1)))),
            _ => ty,
        }
    }
}

/// The signature of `function` with generic arguments `args`, its
/// associated types normalized: an instance's, for code generation.
pub fn instance_sig(
    items: &ItemTree,
    sigs: &Signatures,
    function: FunctionId,
    args: &[Ty],
) -> crate::FnSig {
    let sig = &sigs[function];
    crate::FnSig {
        params: sig
            .params
            .iter()
            .map(|&p| instance_ty(items, sigs, p, args))
            .collect(),
        ret: instance_ty(items, sigs, sig.ret, args),
    }
}

/// `ty` with generic arguments `args` substituted and its associated
/// types normalized: a type of an instance, for code generation.
pub fn instance_ty(items: &ItemTree, sigs: &Signatures, ty: Ty, args: &[Ty]) -> Ty {
    normalize(
        items,
        sigs,
        &[],
        reveal(sigs, normalize(items, sigs, &[], ty.subst(args))),
    )
}

/// The `impl` block for `goal` and its generic arguments: the only one
/// whose header matches and whose bounds can hold, as rustc's selection
/// and winnowing find it (`impl<'a, T> IntoIterator for &'a [T]` rather
/// than the blanket one for iterators); `None` where none or several
/// can, which leaves an associated type of it unnormalized.
///
/// Bounds on generic parameters (and associated types) may hold where
/// the bounds in scope say so, which this does not know, so a block
/// whose failing bounds are all on them can still be the one, for a type
/// whose own shape picks it (`impl<'a, T: Copy> IntoIterator for &'a
/// Bag<T>` for `&Bag<T>`). Not for a parameter itself, though: a bound
/// in scope on it decides rather than a blanket block (`<U as
/// IntoIterator>::IntoIter` for `U: IntoIterator` is not `U`), so there
/// a block's bounds must hold whatever the bounds in scope.
pub fn select_impl(
    items: &ItemTree,
    sigs: &Signatures,
    goal: Predicate,
) -> Option<(ImplId, Vec<Ty>)> {
    let impls = sigs
        .impl_index
        .impls_for(items, goal.trait_ref.trait_, goal.self_ty);
    let mut found: Vec<(ImplId, Vec<Ty>)> = impls
        .filter_map(|imp| Some((imp, match_impl(items, sigs, imp, goal)?)))
        .collect();
    let rigid_self = is_rigid(goal.self_ty);
    // Judging bounds normalizes their associated types, which selects
    // blocks again. A repeated goal cannot establish an inductive proof;
    // growing goals retain the depth limit.
    thread_local! {
        static ACTIVE: std::cell::RefCell<Vec<Predicate>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    if ACTIVE.with_borrow(|active| active.len() > 32 || active.contains(&goal)) {
        return None;
    }
    ACTIVE.with_borrow_mut(|active| active.push(goal));
    // Selection needs only the number and index of eligible candidates.
    // Keep the header-matching and bounds-checking passes in their original
    // order: checking a bound can recursively select another impl.
    let (mut judged_count, mut definite_count) = (0, 0);
    let (mut judged_index, mut definite_index) = (0, 0);
    for (index, (imp, args)) in found.iter().enumerate() {
        if let Some(definite) = impl_bounds(items, sigs, *imp, args, 0) {
            judged_count += 1;
            judged_index = index;
            if definite {
                definite_count += 1;
                definite_index = index;
            }
        }
    }
    ACTIVE.with_borrow_mut(|active| active.pop());
    let chosen = if definite_count == 1 {
        definite_index
    } else if !rigid_self && definite_count == 0 && judged_count == 1 {
        judged_index
    } else {
        return None;
    };
    Some(found.swap_remove(chosen))
}

/// A type that only the bounds in scope say anything about: a generic
/// parameter, an associated type of one, or one not inferred yet.
pub fn is_rigid(ty: Ty) -> bool {
    matches!(
        ty.kind(),
        TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) | TyKind::Infer(_)
    )
}

/// Whether the bounds of `imp`, with generic arguments `args`, hold:
/// `Some(true)` if they surely do, `Some(false)` if they may (where
/// bounds in scope would say), `None` if one cannot.
fn impl_bounds(
    items: &ItemTree,
    sigs: &Signatures,
    imp: ImplId,
    args: &[Ty],
    depth: u32,
) -> Option<bool> {
    let preds = sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
    let mut definite = true;
    for p in preds {
        definite &= may_hold(items, sigs, p.subst(args), depth + 1)?;
    }
    Some(definite)
}

/// Whether `p` holds without the bounds in scope, as [`impl_bounds`]
/// says: surely, if [`holds`] says so; perhaps, if it is about a type
/// only bounds in scope know (`T: Copy`), or about a type with generic
/// parameters in it that an `impl` block whose bounds may hold is for
/// (`&'a Bag<T>: IntoIterator` where `impl<'a, T: Copy> IntoIterator for
/// &'a Bag<T>`); not at all otherwise (`&Bag<T>: Iterator`).
fn may_hold(items: &ItemTree, sigs: &Signatures, p: Predicate, depth: u32) -> Option<bool> {
    if holds(items, sigs, &[], p) {
        return Some(true);
    }
    // A concrete header can contain associated outputs introduced by another
    // bound. Normalize those inputs after the direct proof, so generic and
    // recursive proofs do not eagerly expand every candidate's projections.
    let concrete = |ty: Ty| !ty.has_params() && !ty.has_infer() && !ty.has_opaques();
    if concrete(p.self_ty) && p.trait_ref.args.iter().copied().all(concrete) {
        let normalized = normalize_predicate(items, sigs, p);
        if normalized != p && holds(items, sigs, &[], normalized) {
            return Some(true);
        }
    }
    let open = |t: Ty| t.has_params() || t.has_aliases() || t.has_infer();
    if !open(p.self_ty) && !p.trait_ref.args.iter().any(|&a| open(a)) {
        return None;
    }
    if is_rigid(p.self_ty) || depth > 8 {
        return Some(false);
    }
    if matches!(
        p.self_ty.kind(),
        TyKind::Closure(..) | TyKind::FnDef(_) | TyKind::FnPtr(_)
    ) {
        return Some(false);
    }
    let impls = sigs
        .impl_index
        .impls_for(items, p.trait_ref.trait_, p.self_ty);
    let mut found = None;
    for imp in impls {
        let Some(args) = match_impl(items, sigs, imp, p) else {
            continue;
        };
        if impl_bounds(items, sigs, imp, &args, depth).is_some() {
            found = Some(false);
        }
    }
    // A trait's bound on a parameter in the other arguments
    // (`i32: PartialEq<T>`) may hold where the bounds in scope say.
    if found.is_none() && p.trait_ref.args.iter().any(|&a| is_rigid(a)) {
        return Some(false);
    }
    found
}

/// The generic arguments for which `impl` block `imp`'s header is
/// `goal`, if there are any.
pub fn match_impl(
    items: &ItemTree,
    sigs: &Signatures,
    imp: ImplId,
    goal: Predicate,
) -> Option<Vec<Ty>> {
    match_impl_sized(items, sigs, imp, goal, true)
}

/// [`match_impl`], checking that its parameters not bounded `?Sized`
/// are sized only if `sized`.
pub fn match_impl_sized(
    items: &ItemTree,
    sigs: &Signatures,
    imp: ImplId,
    goal: Predicate,
    sized: bool,
) -> Option<Vec<Ty>> {
    let trait_ref = sigs.impl_traits.get(imp)?;
    if trait_ref.trait_ != goal.trait_ref.trait_ {
        return None;
    }
    let self_ty = *sigs.impls.get(imp)?;
    // Most candidates are rejected. Do not allocate bindings just to reject
    // a short header; only successful matches need an owned argument list.
    let count = items.impls[imp].generics.len();
    let mut short = [None; 8];
    let mut long = Vec::new();
    let binds = if count <= short.len() {
        &mut short[..count]
    } else {
        long.resize(count, None);
        &mut long[..]
    };
    if !match_ty(self_ty, goal.self_ty, binds) {
        return None;
    }
    for (&x, &y) in trait_ref.args.iter().zip(goal.trait_ref.args) {
        // Matching Self may already fix the input of a projection in a
        // trait argument (`A::Item` for `SmallVec<A>`). Normalize it using
        // those bindings before matching the remaining header.
        if !match_impl_argument(items, sigs, x, y, binds, 0) {
            return None;
        }
    }
    bind_by_projections(items, sigs, imp, binds);
    // Its parameters are `Sized` unless bounded `?Sized`.
    let params = &items.impls[imp].generics.params;
    if sized
        && params
            .iter()
            .zip(binds.iter())
            .any(|(p, b)| !p.maybe_unsized && b.is_some_and(|ty| sigs.is_unsized(ty)))
    {
        return None;
    }
    let args: Vec<_> = binds.iter().map(|b| b.unwrap_or(Ty::ERROR)).collect();
    if !crate::fn_regions::same_binders(items, self_ty.subst(&args), goal.self_ty)
        || trait_ref
            .args
            .iter()
            .zip(goal.trait_ref.args)
            .any(|(&a, &b)| !crate::fn_regions::same_binders(items, a.subst(&args), b))
    {
        return None;
    }
    Some(args)
}

/// Match a trait argument without confusing the impl's parameters with the
/// caller's rigid parameters after normalizing a bound projection.
fn match_impl_argument(
    items: &ItemTree,
    sigs: &Signatures,
    pattern: Ty,
    actual: Ty,
    binds: &mut [Option<Ty>],
    depth: u32,
) -> bool {
    if depth > 64 {
        return false;
    }
    if !pattern.has_aliases() {
        return match_ty(pattern, actual, binds);
    }
    fn bound(ty: Ty, binds: &[Option<Ty>]) -> bool {
        match ty.kind() {
            TyKind::Param(parameter) => binds
                .get(parameter.index as usize)
                .is_some_and(Option::is_some),
            _ => ty.children().all(|ty| bound(ty, binds)),
        }
    }
    if matches!(pattern.kind(), TyKind::Alias(_)) && bound(pattern, binds) {
        let args: Vec<_> = binds.iter().map(|ty| ty.unwrap_or(Ty::ERROR)).collect();
        let normalized = normalize(items, sigs, &[], pattern.subst(&args));
        return normalized == actual;
    }
    let mut all = |left: &[Ty], right: &[Ty]| {
        left.len() == right.len()
            && left.iter().zip(right).all(|(&left, &right)| {
                match_impl_argument(items, sigs, left, right, binds, depth + 1)
            })
    };
    match (pattern.kind(), actual.kind()) {
        (TyKind::Adt(left, xs), TyKind::Adt(right, ys)) => left == right && all(xs, ys),
        (TyKind::Tuple(xs), TyKind::Tuple(ys)) => all(xs, ys),
        (TyKind::Ref(left, x), TyKind::Ref(right, y))
        | (TyKind::RawPtr(left, x), TyKind::RawPtr(right, y)) => {
            left == right && match_impl_argument(items, sigs, x, y, binds, depth + 1)
        }
        (TyKind::Slice(x), TyKind::Slice(y)) => {
            match_impl_argument(items, sigs, x, y, binds, depth + 1)
        }
        (TyKind::Array(x, n), TyKind::Array(y, m)) => {
            match_ty(n, m, binds) && match_impl_argument(items, sigs, x, y, binds, depth + 1)
        }
        _ => match_ty(pattern, actual, binds),
    }
}

/// The parameters of `impl` block `imp` that only its bounds' associated
/// types decide (`B` in `where F: FnMut(I::Item) -> B`), from what those
/// types are for the ones `binds` has.
pub(crate) fn bind_by_projections(
    items: &ItemTree,
    sigs: &Signatures,
    imp: ImplId,
    binds: &mut [Option<Ty>],
) {
    let projections = sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
    if binds.iter().all(Option::is_some) || projections.is_empty() {
        return;
    }
    loop {
        let mut progress = false;
        for p in projections {
            let unbound = |binds: &[Option<Ty>]| {
                (0..binds.len())
                    .any(|i| binds[i].is_none() && crate::mentions_param(p.ty, i as u32))
            };
            if !unbound(binds) {
                continue;
            }
            let known: Vec<Ty> = binds.iter().map(|b| b.unwrap_or(Ty::ERROR)).collect();
            let alias = Ty::alias(p.alias).subst(&known);
            // The block for its own parameters: the parameters themselves.
            let ty = if alias == Ty::alias(p.alias) {
                p.ty
            } else if has_error(alias) {
                continue;
            } else {
                // Bound inputs may contain the caller's rigid parameters.
                // They are values of this substitution, not unbound impl
                // variables; e.g. IntoIter<T>::Item still normalizes to T.
                normalize(items, sigs, &[], alias)
            };
            if matches!(ty.kind(), TyKind::Alias(_)) {
                continue;
            }
            let mut found = binds.to_vec();
            if match_ty(p.ty, ty, &mut found) {
                binds.copy_from_slice(&found);
                progress = true;
            }
        }
        if !progress {
            return;
        }
    }
}

pub fn has_error(ty: Ty) -> bool {
    ty.has_error()
}

/// Whether `goal` holds given `env`, the elaborated bounds in scope, in
/// which generic parameters are rigid: from a bound, or from an `impl`
/// block whose own bounds hold.
pub fn holds(items: &ItemTree, sigs: &Signatures, env: &[Predicate], goal: Predicate) -> bool {
    Holds {
        items,
        sigs,
        env,
        stack: Vec::new(),
        memo: rustc_hash::FxHashMap::default(),
        cut: usize::MAX,
    }
    .holds_at(goal)
}

/// Auto-trait cycles are coinductive only while every edge in the cycle
/// remains an auto trait. Ordinary trait requirements stay inductive.
fn auto_cycle(items: &ItemTree, goal: Predicate, stack: &[Predicate]) -> Option<(usize, bool)> {
    let start = stack.iter().position(|p| *p == goal)?;
    Some((
        start,
        stack[start..]
            .iter()
            .all(|p| is_auto_trait(items, p.trait_ref.trait_)),
    ))
}

/// One [`holds`]: the goals it is proving, and the answers for those it
/// proved. Types are DAGs (`struct A(B, B)`), so a nested goal recurs
/// often; each is proved once.
struct Holds<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    env: &'a [Predicate],
    stack: Vec<Predicate>,
    /// Answers that did not depend on a goal further down `stack`.
    memo: rustc_hash::FxHashMap<Predicate, bool>,
    /// The lowest index in `stack` that a cycle or the depth limit cut
    /// at, since the goal being proved started: an answer whose proof
    /// cut below its own goal's index depends on its callers.
    cut: usize,
}

impl Holds<'_> {
    /// [`holds`], within the goals `stack` is proving. As in rustc, a goal
    /// that needs itself does not hold, nor does one that needs ever larger
    /// types.
    fn holds_at(&mut self, goal: Predicate) -> bool {
        if let Some(&known) = self.memo.get(&goal) {
            return known;
        }
        let depth = self.stack.len();
        let outer = std::mem::replace(&mut self.cut, usize::MAX);
        let holds = self.prove(goal);
        // The outermost goal is not asked again.
        if self.cut >= depth && depth > 0 {
            self.memo.insert(goal, holds);
        }
        self.cut = self.cut.min(outer);
        holds
    }

    fn nested(&mut self, goal: Predicate, nested: Predicate) -> bool {
        self.stack.push(goal);
        let holds = self.holds_at(nested);
        self.stack.pop();
        holds
    }

    fn prove(&mut self, goal: Predicate) -> bool {
        let (items, sigs, env) = (self.items, self.sigs, self.env);
        if goal.self_ty == Ty::ERROR || goal.trait_ref.args.contains(&Ty::ERROR) {
            return true;
        }
        if in_env(sigs, env, goal) {
            return true;
        }
        if env
            .iter()
            .any(|p| p.trait_ref.trait_ == goal.trait_ref.trait_)
        {
            let normalized = normalize_predicate(items, sigs, goal);
            if env.iter().any(|&predicate| {
                predicate.trait_ref.trait_ == goal.trait_ref.trait_
                    && normalize_predicate(items, sigs, predicate) == normalized
            }) {
                return true;
            }
        }
        if let Some((start, coinductive)) = auto_cycle(items, goal, &self.stack) {
            self.cut = self.cut.min(start);
            return coinductive;
        }
        if self.stack.len() > 256 {
            self.cut = 0;
            return false;
        }
        if let Some(owner) = declared_alias_bound(sigs, goal)
            && self.nested(goal, owner)
        {
            return true;
        }
        // Outside bodies, a parameter is taken to be sized.
        if items.traits[goal.trait_ref.trait_].lang == Some(flux_intern::sym::sized) {
            return !crate::maybe_unsized(items, sigs, goal.self_ty, &[], env);
        }
        if is_builtin_trait(items, goal.trait_ref.trait_, goal.self_ty) {
            return true;
        }
        if let Some(found) = fn_goal(items, goal.trait_ref.trait_, goal.self_ty) {
            return match found {
                ClosureGoal::Fn(_, params) => goal.trait_ref.args.first() == Some(&params),
                _ => false,
            };
        }
        if let TyKind::Closure(_, args) = goal.self_ty.kind() {
            match closure_goal(items, goal.trait_ref.trait_, args) {
                ClosureGoal::Fn(kind, params) => {
                    return ClosureArgs::split(args)
                        .closure_kind()
                        .is_some_and(|k| k <= kind)
                        && goal.trait_ref.args.first() == Some(&params);
                }
                ClosureGoal::Upvars(upvars) => {
                    let trait_ref = copy_trait_ref(items).unwrap_or(goal.trait_ref);
                    return upvars.iter().all(|&ty| {
                        let upvar = Predicate {
                            self_ty: ty,
                            trait_ref,
                        };
                        self.nested(goal, upvar)
                    });
                }
                // Other traits hold through blanket `impl` blocks, below.
                ClosureGoal::No => {}
            }
        }
        let mut impls = sigs
            .impl_index
            .impls_for(items, goal.trait_ref.trait_, goal.self_ty);
        let holds = impls.any(|imp| {
            let Some(args) = match_impl(items, sigs, imp, goal) else {
                return false;
            };
            let nested = sigs.impl_predicates.get(imp).into_iter().flatten();
            nested
                .copied()
                .all(|p| self.nested(goal, normalize_predicate(items, sigs, p.subst(&args))))
        });
        if holds {
            return true;
        }
        let Some(nested) = auto_trait_requirements(items, sigs, goal) else {
            return false;
        };
        nested
            .into_iter()
            .all(|p| self.nested(goal, normalize_predicate(items, sigs, p)))
    }
}

/// Built-in auto traits, identified by core's lang items, never by name.
pub fn is_auto_trait(items: &ItemTree, t: TraitId) -> bool {
    items.traits[t].lang.is_some_and(|l| {
        matches!(
            l.as_str(),
            "send" | "sync" | "unpin" | "unwind_safe" | "ref_unwind_safe"
        )
    })
}

/// Structural auto-trait obligations. Explicit impls replace structural
/// propagation for the entire ADT, including instantiations whose impl
/// bounds fail. Unknown parameters and raw pointers have no candidate.
/// Exact cycles are coinductive; growing instantiations retain depth limits.
pub fn auto_trait_requirements(
    items: &ItemTree,
    sigs: &Signatures,
    goal: Predicate,
) -> Option<Vec<Predicate>> {
    let t = goal.trait_ref.trait_;
    if !is_auto_trait(items, t) {
        return None;
    }
    let mut trait_ref = goal.trait_ref;
    let unwind_safe = items.traits[t]
        .lang
        .is_some_and(|l| l.as_str() == "unwind_safe");
    let ref_unwind_safe = items.traits[t]
        .lang
        .is_some_and(|l| l.as_str() == "ref_unwind_safe");
    let types = match goal.self_ty.kind() {
        TyKind::Int(_)
        | TyKind::Float(_)
        | TyKind::Bool
        | TyKind::Char
        | TyKind::Str
        | TyKind::Never
        | TyKind::CStr
        | TyKind::FnDef(_)
        | TyKind::FnPtr(_) => Vec::new(),
        TyKind::Ref(_, _) | TyKind::RawPtr(_, _)
            if items.traits[t].lang.is_some_and(|l| l.as_str() == "unpin") =>
        {
            Vec::new()
        }
        TyKind::Dynamic(object) if object.auto_traits.iter().any(|bound| bound.trait_ == t) => {
            Vec::new()
        }
        TyKind::Tuple(types) => types.to_vec(),
        TyKind::Array(ty, _) | TyKind::Slice(ty) => vec![ty],
        TyKind::Ref(Mutability::Mut, _) if unwind_safe => return None,
        TyKind::Ref(_, ty) | TyKind::RawPtr(_, ty) if unwind_safe => {
            let shared = *items.lang_traits.get(&Symbol::intern("ref_unwind_safe"))?;
            trait_ref = TraitRef {
                trait_: shared,
                name: items.traits[shared].name,
                args: Ty::list(&[]),
            };
            vec![ty]
        }
        TyKind::Ref(_, ty) | TyKind::RawPtr(_, ty) if ref_unwind_safe => vec![ty],
        TyKind::Ref(m, ty) => {
            // &T: Send requires T: Sync; &mut T: Send requires T: Send.
            // Both reference kinds are Sync exactly when T is Sync.
            if m == Mutability::Not {
                let sync = *items.lang_traits.get(&Symbol::intern("sync"))?;
                trait_ref = TraitRef {
                    trait_: sync,
                    name: items.traits[sync].name,
                    args: Ty::list(&[]),
                };
            }
            vec![ty]
        }
        // Keep the tuple itself as an obligation: during inference it may
        // still be a variable, and must not look like an empty capture list.
        TyKind::Closure(_, args) => vec![ClosureArgs::split(args).upvars],
        TyKind::Adt(adt, args) => {
            if items.trait_impls.get(&t).into_iter().flatten().any(|&imp| {
                matches!(sigs.impls.get(imp).map(|ty| ty.kind()), Some(TyKind::Adt(a, _)) if a.strukt == adt.strukt)
            }) {
                return None;
            }
            if crate::is_lang_struct(items, adt.strukt, "phantom_pinned")
                && items.traits[t].lang.is_some_and(|l| l.as_str() == "unpin")
            {
                return None;
            }
            // The direct interior-mutability primitives are never structurally Sync.
            if (crate::is_lang_struct(items, adt.strukt, "cell")
                || crate::is_lang_struct(items, adt.strukt, "unsafe_cell"))
                && items.traits[t].lang.is_some_and(|l| l.as_str() == "sync")
            {
                return None;
            }
            // Cell stores T directly in this facade, but has UnsafeCell's
            // shared-mutation contract just like Rust's representation.
            if (crate::is_lang_struct(items, adt.strukt, "unsafe_cell")
                || crate::is_lang_struct(items, adt.strukt, "cell"))
                && ref_unwind_safe
            {
                return None;
            }
            if crate::is_lang_struct(items, adt.strukt, "phantom_data") {
                args.to_vec()
            } else {
                sigs.structs[adt.strukt]
                    .iter()
                    .map(|f| f.subst(args))
                    .collect()
            }
        }
        _ => return None,
    };
    Some(
        types
            .into_iter()
            .map(|self_ty| Predicate { self_ty, trait_ref })
            .collect(),
    )
}

/// `p` with the associated types of known types in it normalized.
pub fn normalize_predicate(items: &ItemTree, sigs: &Signatures, p: Predicate) -> Predicate {
    if !p.self_ty.has_aliases() && !p.trait_ref.args.iter().any(|a| a.has_aliases()) {
        return p;
    }
    let args = crate::ty::map_list(p.trait_ref.args, |a| normalize(items, sigs, &[], a));
    Predicate {
        self_ty: normalize(items, sigs, &[], p.self_ty),
        trait_ref: TraitRef {
            args,
            ..p.trait_ref
        },
    }
}

/// Whether `ty: t` holds by the compiler's say, including `Copy` and
/// `Clone` for function items and pointers of every supported signature.
pub fn is_builtin_copy(items: &ItemTree, t: TraitId, ty: Ty) -> bool {
    let lang = items.traits[t].lang;
    match ty.kind() {
        // `!` is too, as rustc's `core` has it, which the hidden type
        // of a function that only calls itself is.
        TyKind::Ref(Mutability::Not, _) | TyKind::Never => {
            matches!(lang, Some(l) if l.as_str() == "copy" || l.as_str() == "clone")
        }
        // Cloning copies the value without calling it or relating its
        // signature's types or higher-ranked input lifetimes.
        TyKind::FnDef(_) | TyKind::FnPtr(_) => {
            matches!(lang, Some(l) if l.as_str() == "copy" || l.as_str() == "clone")
        }
        _ => false,
    }
}

/// Built-in trait implementations with no user impl body to select.
pub fn is_builtin_trait(items: &ItemTree, t: TraitId, ty: Ty) -> bool {
    is_builtin_copy(items, t, ty)
        || (matches!(ty.kind(), TyKind::FnPtr(_))
            && items.traits[t]
                .lang
                .is_some_and(|lang| lang.as_str() == "Debug"))
}

/// Whether values of `ty` are copied rather than moved, given `env`, the
/// elaborated bounds in scope: those of types that implement `Copy`.
pub fn is_copy(items: &ItemTree, sigs: &Signatures, env: &[Predicate], ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Tuple(elements) => elements.iter().all(|&t| is_copy(items, sigs, env, t)),
        TyKind::Array(elem, _) => is_copy(items, sigs, env, elem),
        TyKind::Slice(_) | TyKind::Str => false,
        TyKind::Closure(_, args) => ClosureArgs::split(args)
            .upvar_types()
            .iter()
            .all(|&t| is_copy(items, sigs, env, t)),
        TyKind::Ref(Mutability::Mut, _) => false,
        TyKind::Adt(..) | TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) => {
            let Some(&copy) = items.lang_traits.get(&flux_intern::sym::copy) else {
                return false;
            };
            let goal = Predicate {
                self_ty: ty,
                trait_ref: TraitRef {
                    trait_: copy,
                    name: items.traits[copy].name,
                    args: Ty::list(&[]),
                },
            };
            holds(items, sigs, env, goal)
        }
        _ => true,
    }
}

/// What a goal on a closure's type needs, by its trait: its kind and
/// parameters for a `Fn`-family trait, its captured values' meeting it
/// for `Copy` and `Clone`; other traits it does not implement.
pub enum ClosureGoal {
    Fn(crate::ClosureKind, Ty),
    Upvars(&'static [Ty]),
    No,
}

/// What a goal on a function item or pointer type needs: a safe
/// function implements the `Fn`-family traits for its parameters.
pub fn fn_goal(items: &ItemTree, t: TraitId, ty: Ty) -> Option<ClosureGoal> {
    let sig = ty.fn_sig()?;
    let kind = crate::fn_trait_kind(items, t)?;
    if sig.is_unsafe
        || sig.is_c
        || matches!(ty.kind(),TyKind::FnDef(def) if items.functions[def.function].target_features.is_some())
    {
        return Some(ClosureGoal::No);
    }
    Some(ClosureGoal::Fn(kind, Ty::tuple(sig.params())))
}

pub fn closure_goal(items: &ItemTree, t: TraitId, args: GenericArgs) -> ClosureGoal {
    let parts = ClosureArgs::split(args);
    if let Some(kind) = crate::fn_trait_kind(items, t) {
        return ClosureGoal::Fn(kind, parts.params());
    }
    // A closure is `Clone` only where it is `Copy`, cloned by a copy.
    match items.traits[t].lang {
        Some(l) if l.as_str() == "copy" || l.as_str() == "clone" => {
            ClosureGoal::Upvars(parts.upvar_types())
        }
        _ => ClosureGoal::No,
    }
}

/// `Copy`, which what a closure captures must be for it to be `Copy` or
/// `Clone`.
pub fn copy_trait_ref(items: &ItemTree) -> Option<TraitRef> {
    let copy = *items.lang_traits.get(&flux_intern::sym::copy)?;
    Some(TraitRef {
        trait_: copy,
        name: items.traits[copy].name,
        args: Ty::list(&[]),
    })
}

/// `<C as FnOnce<A>>::Output` for a closure `C`: what it returns.
pub fn closure_output(items: &ItemTree, alias: AliasTy) -> Option<Ty> {
    if let Some(sig) = alias.args[0].fn_sig()
        && !sig.is_unsafe
        && !sig.is_c
        && !matches!(alias.args[0].kind(),TyKind::FnDef(def) if items.functions[def.function].target_features.is_some())
        && crate::fn_trait_kind(items, alias.trait_) == Some(crate::ClosureKind::FnOnce)
    {
        return Some(sig.ret());
    }
    let TyKind::Closure(_, args) = alias.args[0].kind() else {
        return None;
    };
    (crate::fn_trait_kind(items, alias.trait_) == Some(crate::ClosureKind::FnOnce))
        .then(|| ClosureArgs::split(args).ret())
}

/// The `impl Drop` block of `strukt`, if it has one.
pub fn drop_impl(items: &ItemTree, sigs: &Signatures, strukt: StructId) -> Option<ImplId> {
    let drop = *items.lang_traits.get(&flux_intern::sym::drop)?;
    let head = Some(crate::impl_index::Head::Adt(strukt));
    sigs.impl_index.impls_with_head(items, drop, head).find(|&imp| {
        matches!(sigs.impls.get(imp).map(|t| t.kind()), Some(TyKind::Adt(adt, _)) if adt.strukt == strukt)
    })
}

/// Whether dropping a `ty` runs code, given `env`, the elaborated bounds
/// in scope: it is, or has a part that is, a struct with a `Drop` impl,
/// or a type parameter that is not `Copy`, which may be one.
pub fn needs_drop(items: &ItemTree, sigs: &Signatures, env: &[Predicate], ty: Ty) -> bool {
    DropWalk {
        items,
        sigs,
        env,
        structs: 0,
        memo: rustc_hash::FxHashMap::default(),
    }
    .needs_drop(ty)
}

/// One [`needs_drop`]. A type is a DAG (`struct A(B, B)`), which a walk
/// that does not remember the structs it met sees as a tree; most walks
/// meet a few, for which a map costs more than it saves.
struct DropWalk<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    env: &'a [Predicate],
    /// How many structs the walk met: after [`DropWalk::UNREMEMBERED`],
    /// it remembers them in `memo`.
    structs: u32,
    memo: rustc_hash::FxHashMap<Ty, bool>,
}

impl DropWalk<'_> {
    const UNREMEMBERED: u32 = 16;

    fn needs_drop(&mut self, ty: Ty) -> bool {
        let (items, sigs, env) = (self.items, self.sigs, self.env);
        match ty.kind() {
            TyKind::Tuple(elements) => elements.iter().any(|&e| self.needs_drop(e)),
            TyKind::Array(elem, len) => len.as_usize() != Some(0) && self.needs_drop(elem),
            TyKind::Slice(elem) => self.needs_drop(elem),
            // The erased concrete type may have a destructor.
            TyKind::Dynamic(_) => true,
            TyKind::Closure(_, args) => ClosureArgs::split(args)
                .upvar_types()
                .iter()
                .any(|&e| self.needs_drop(e)),
            // Holds a value it never drops.
            TyKind::Adt(adt, _) if crate::is_lang_struct(items, adt.strukt, "manually_drop") => {
                false
            }
            TyKind::Adt(adt, args) => {
                self.structs += 1;
                let remember = self.structs > Self::UNREMEMBERED;
                if remember && let Some(&known) = self.memo.get(&ty) {
                    return known;
                }
                // Copy excludes destructors even when a stored associated
                // type has not been normalized to its concrete
                // representation yet.
                let needs = !is_copy(items, sigs, env, ty)
                    && (drop_impl(items, sigs, adt.strukt).is_some()
                        || sigs.structs[adt.strukt]
                            .iter()
                            .any(|f| self.needs_drop(f.subst(args))));
                if remember {
                    self.memo.insert(ty, needs);
                }
                needs
            }
            TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) => {
                !is_copy(items, sigs, env, ty)
            }
            _ => false,
        }
    }
}

/// Whether `function` is a destructor: `Drop::drop`, or an `impl Drop`
/// block's `drop`, which may not be called directly.
pub fn is_destructor(items: &ItemTree, function: FunctionId) -> bool {
    let data = &items.functions[function];
    let t = match data.owner {
        Some(Owner::Trait(t)) => Some(t),
        Some(Owner::Impl(imp)) => items.impls[imp].trait_,
        None => None,
    };
    t.and_then(|t| items.traits[t].lang)
        .is_some_and(|lang| lang == flux_intern::sym::drop)
}

/// The function a call of `instance` calls: for a trait's method with
/// known types, the method of the `impl` block for them, or the trait's
/// own default; any other function is itself.
pub fn resolve_instance(items: &ItemTree, sigs: &Signatures, instance: Instance) -> Instance {
    let instance = Instance {
        args: crate::ty::map_list(instance.args, |a| normalize(items, sigs, &[], a)),
        ..instance
    };
    let data = &items.functions[instance.function];
    let Some(Owner::Trait(t)) = data.owner else {
        return instance;
    };
    // An object's principal and supertrait methods dispatch through its
    // vtable, even when a blanket impl also matches the erased object type.
    if instance.args.first().is_some_and(|ty| {
        matches!(ty.kind(), TyKind::Dynamic(d)
        if crate::objects::supertraits_of(items, d.trait_).contains(&t))
    }) {
        return instance;
    }
    let count = 1 + items.traits[t].generics.len();
    let (header, own) = instance.args.split_at(count.min(instance.args.len()));
    let goal = Predicate {
        self_ty: header[0],
        trait_ref: TraitRef {
            trait_: t,
            name: items.traits[t].name,
            args: Ty::list(&header[1..]),
        },
    };
    if let Some((imp, mut args)) = select_impl(items, sigs, goal) {
        let found = items.impls[imp]
            .functions
            .iter()
            .copied()
            .find(|&f| items.functions[f].name == data.name);
        if let Some(function) = found {
            args.extend_from_slice(own);
            return Instance {
                function,
                args: Ty::list(&args),
            };
        }
        // The trait's default.
        return instance;
    }
    instance
}

/// Whether method `f` takes `self` by value (not `&self` or `&mut
/// self`).
pub fn self_by_value(items: &ItemTree, f: FunctionId) -> bool {
    let function = &items.functions[f];
    function.has_self
        && !matches!(
            function.params.first().map(|p| &p.ty),
            Some(flux_hir::TypeRef::Ref { .. })
        )
}

/// The constant a path to `c` names once its generic arguments are
/// known: for a trait's, the `impl` block's for them, or the trait's
/// default; any other constant is itself.
pub fn resolve_const(items: &ItemTree, sigs: &Signatures, c: ConstInstance) -> ConstInstance {
    let args = crate::ty::map_list(c.args, |a| normalize(items, sigs, &[], a));
    let c = ConstInstance { args, ..c };
    let data = &items.consts[c.konst];
    let (Some(Owner::Trait(t)), Some(name)) = (data.owner, data.name) else {
        return c;
    };
    let goal = Predicate {
        self_ty: args[0],
        trait_ref: TraitRef {
            trait_: t,
            name: items.traits[t].name,
            args: Ty::list(&args[1..]),
        },
    };
    if let Some((imp, impl_args)) = select_impl(items, sigs, goal) {
        let found = items.impls[imp]
            .consts
            .iter()
            .copied()
            .find(|&k| items.consts[k].name == Some(name));
        return match found {
            Some(konst) => ConstInstance {
                konst,
                args: Ty::list(&impl_args),
            },
            None => c,
        };
    }
    c
}

/// Whether a type contains interior mutability, independently of the active
/// variant or array length. References and raw pointers describe pointer slots,
/// not owned pointees. Static reads use this type-level property; promotion
/// validation separately inspects the actual value.
pub fn has_interior_mutability(items: &ItemTree, sigs: &Signatures, ty: Ty) -> bool {
    fn visit(
        items: &ItemTree,
        sigs: &Signatures,
        ty: Ty,
        seen: &mut rustc_hash::FxHashSet<Ty>,
    ) -> bool {
        let ty = normalize(items, sigs, &[], ty);
        if !seen.insert(ty) {
            return false;
        }
        match ty.kind() {
            TyKind::Adt(adt, _)
                if ["cell", "ref_cell", "unsafe_cell"]
                    .iter()
                    .any(|lang| crate::is_lang_struct(items, adt.strukt, lang)) =>
            {
                true
            }
            TyKind::Array(elem, _) => visit(items, sigs, elem, seen),
            TyKind::Tuple(_) | TyKind::Adt(..) | TyKind::Closure(..) => crate::contents(sigs, ty)
                .is_some_and(|fields| fields.iter().any(|&t| visit(items, sigs, t, seen))),
            _ => false,
        }
    }
    visit(items, sigs, ty, &mut rustc_hash::FxHashSet::default())
}
