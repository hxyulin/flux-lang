//! The declaration representation boundary used by trait-lifetime admission.
use flux_hir::{FunctionId, Generics, ItemTree, Owner, TraitId, TypeRef};

use super::{Elided, LifetimeName, Outlives, Region, Walker, implied_outlives_with_static};
use crate::{Signatures, Ty, TyKind, TypeScope};

/// Why a declaration does not have the bounded required-method profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraitProfileReason {
    MissingMetadata,
    DeclarationDiagnostics,
    OwnerLifetimes,
    TraitParameters,
    TraitPredicates,
    AssociatedItems,
    UnsafeTrait,
    MethodShape,
    MethodGenerics,
    SignatureType,
    OwnerMetadata,
    SignatureMetadata,
    SupertraitRegions,
    SupertraitMetadata,
    SupertraitCycle,
}

/// The first unsupported structure, with its declaration coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraitProfileError {
    pub trait_: TraitId,
    pub method: Option<FunctionId>,
    pub reason: TraitProfileReason,
}

/// Check the initial one-owner-lifetime, required-safe-method representation.
///
/// Admission uses this query after all signature metadata has been resolved.
/// `Ok(())` is not a proof of a trait obligation or of supported use contexts.
/// All syntax/HIR/type diagnostics remain independently mandatory.
/// Admission retains signature WF, impl compatibility, concrete and
/// caller region obligations, associated declaration checks and all existing
/// use-site guards (closures, default bodies, inherited regions, abstract
/// blanket forwarding, named function items and higher-ranked owner items).
/// Signatures exclude function pointers, opaque types and qualified or unresolved
/// associated projections. Non-callable objects retain independent bound slots
/// only when their principal/associated arguments are closed and region-free.
/// General region-free traits reached through supertrait edges are checked for
/// region-free edges, not subjected to this required-method profile.
pub fn required_method_trait_profile(
    hir: &flux_hir::Hir,
    sigs: &Signatures,
    trait_: TraitId,
) -> Result<(), TraitProfileError> {
    required_method_profile(&hir.items, &hir.diagnostics, sigs, trait_)
}

pub(crate) fn required_method_profile(
    items: &ItemTree,
    diagnostics: &[flux_hir::Diagnostic],
    sigs: &Signatures,
    trait_: TraitId,
) -> Result<(), TraitProfileError> {
    let fail = |reason| TraitProfileError {
        trait_,
        method: None,
        reason,
    };
    let t = items
        .traits
        .get(trait_)
        .ok_or(fail(TraitProfileReason::MissingMetadata))?;
    if t.generics.lifetimes.len() != 1 {
        return Err(fail(TraitProfileReason::OwnerLifetimes));
    }
    if !t.generics.params.is_empty() {
        return Err(fail(TraitProfileReason::TraitParameters));
    }
    if has_predicates(&t.generics) {
        return Err(fail(TraitProfileReason::TraitPredicates));
    }
    if !t.types.is_empty() || !t.consts.is_empty() {
        return Err(fail(TraitProfileReason::AssociatedItems));
    }
    if t.is_unsafe {
        return Err(fail(TraitProfileReason::UnsafeTrait));
    }
    if diagnosed(diagnostics, t.loc.file, t.def_range) {
        return Err(fail(TraitProfileReason::DeclarationDiagnostics));
    }
    let owner = sigs
        .regions
        .traits
        .get(trait_)
        .ok_or(fail(TraitProfileReason::MissingMetadata))?;
    let declared = &t.generics.lifetimes[0];
    if owner.names.as_ref()
        != [LifetimeName {
            name: Some(declared.name),
            range: declared.range,
        }]
        || !owner.outlives.is_empty()
    {
        return Err(fail(TraitProfileReason::OwnerMetadata));
    }
    // Check the list in both directions: an omitted, repeated or foreign method
    // cannot impersonate a complete set of required contracts.
    let mut listed = rustc_hash::FxHashSet::default();
    for &id in &t.functions {
        let fail = |reason| TraitProfileError {
            trait_,
            method: Some(id),
            reason,
        };
        let f = items
            .functions
            .get(id)
            .ok_or(fail(TraitProfileReason::MissingMetadata))?;
        if !listed.insert(id) || f.owner != Some(Owner::Trait(trait_)) {
            return Err(fail(TraitProfileReason::OwnerMetadata));
        }
        if f.has_body
            || f.is_unsafe
            || f.is_extern
            || f.is_const
            || f.is_variadic
            || f.intrinsic.is_some()
            || f.derived.is_some()
            || f.ctor.is_some()
        {
            return Err(fail(TraitProfileReason::MethodShape));
        }
        if !f.generics.params.is_empty()
            || !f.generics.lifetimes.is_empty()
            || has_predicates(&f.generics)
            || !f.opaques.is_empty()
        {
            return Err(fail(TraitProfileReason::MethodGenerics));
        }
        if !written_type(&f.ret, false) || f.params.iter().any(|p| !written_type(&p.ty, false)) {
            return Err(fail(TraitProfileReason::SignatureType));
        }
        // Some unsupported qualifiers (for example async/extern functions)
        // are diagnosed during lowering and intentionally have no HIR flag.
        // Reject the diagnosed declaration without inspecting syntax or text.
        if diagnosed(diagnostics, f.loc.file, f.item_range) {
            return Err(fail(TraitProfileReason::DeclarationDiagnostics));
        }
        let sig = sigs
            .fns
            .get(id)
            .ok_or(fail(TraitProfileReason::MissingMetadata))?;
        let r = sigs
            .regions
            .fns
            .get(id)
            .ok_or(fail(TraitProfileReason::MissingMetadata))?;
        if r.owner != 1
            || r.names.first() != owner.names.first()
            || r.names.iter().skip(1).any(|n| n.name.is_some())
        {
            return Err(fail(TraitProfileReason::OwnerMetadata));
        }
        if sig.params.len() != f.params.len()
            || r.params.len() != sig.params.len()
            || r.has_bounds
            || r.callback_binder == super::CallbackBinder::Unclassified
            || r.callback_binder != super::resolve::callback_binder(items, sigs, id, r)
            || !r.bound_selfs.is_empty()
            || !r.bound_traits.is_empty()
            || !r.projections.is_empty()
            || !r.opaques.is_empty()
            || sigs.fn_predicates.get(id).is_some_and(|p| !p.is_empty())
            || sigs.fn_projections.get(id).is_some_and(|p| !p.is_empty())
            || sigs.opaques.get(id).is_some_and(|p| !p.is_empty())
        {
            return Err(fail(TraitProfileReason::SignatureMetadata));
        }
        // Reconstruct region tables from HIR and the resolved types. This checks
        // slot counts, binder indices, elision and prefix/name arities together,
        // rather than accepting an in-range but unrelated Param substitution.
        let names = crate::generic_names(items, f.owner, &f.generics);
        let mut w = Walker::new(items, f.loc.module, vec![Some(declared.name)]);
        w.scope = TypeScope {
            self_ty: Some(Ty::param(0, flux_intern::kw::SelfUpper)),
            generics: &names,
            ..TypeScope::default()
        };
        w.elided = Elided::Fresh;
        let lens = crate::ArrayLens {
            values: &sigs.array_lens,
            missing: Default::default(),
            arg_tys: &sigs.arg_tys,
            new_arg_tys: Default::default(),
        };
        let scope = TypeScope {
            signature_of: Some("functions"),
            defaults: Some(&sigs.trait_defaults),
            lens: Some(&lens),
            ..w.scope
        };
        let mut errors = Vec::new();
        for (p, &ty) in f.params.iter().zip(sig.params.iter()) {
            if crate::resolve_type(items, f.loc.module, &p.ty, scope, p.range, &mut errors) != ty {
                return Err(fail(TraitProfileReason::SignatureMetadata));
            }
        }
        if crate::resolve_type(items, f.loc.module, &f.ret, scope, f.ret_range, &mut errors)
            != sig.ret
            || !errors.is_empty()
        {
            return Err(fail(TraitProfileReason::SignatureMetadata));
        }
        let mut distinct = Vec::new();
        let mut outlives = Vec::new();
        for ((p, &ty), table) in f.params.iter().zip(sig.params.iter()).zip(r.params.iter()) {
            if type_slots(items, sigs, ty, 1).is_none() {
                return Err(fail(TraitProfileReason::SignatureType));
            }
            if w.regions(&p.ty, ty).as_ref() != table.as_ref() {
                return Err(fail(TraitProfileReason::SignatureMetadata));
            }
            let mut own = Vec::new();
            for &region in table.iter() {
                if !own.contains(&region) {
                    own.push(region);
                }
            }
            distinct.extend(own);
            implied_outlives_with_static(
                items,
                sigs,
                ty,
                table,
                Some(Region::Static),
                &mut outlives,
            );
        }
        w.elided =
            if f.has_self && matches!(f.params.first().map(|p| &p.ty), Some(TypeRef::Ref { .. })) {
                Elided::Is(
                    r.params
                        .first()
                        .and_then(|p| p.first())
                        .copied()
                        .unwrap_or(Region::Erased),
                )
            } else if distinct.len() == 1 {
                Elided::Is(distinct[0])
            } else {
                Elided::Missing(None)
            };
        if type_slots(items, sigs, sig.ret, 1).is_none() {
            return Err(fail(TraitProfileReason::SignatureType));
        }
        let ret = w.regions(&f.ret, sig.ret);
        implied_outlives_with_static(
            items,
            sigs,
            sig.ret,
            &ret,
            Some(Region::Static),
            &mut outlives,
        );
        outlives.retain(|p| match *p {
            Outlives::Region(a, b) => a != Region::Erased && b != Region::Erased && a != b,
            Outlives::Type(_, b) => b != Region::Erased,
        });
        let mut unique = Vec::new();
        for p in outlives {
            if !unique.contains(&p) {
                unique.push(p);
            }
        }
        let expected_names: Vec<_> = owner
            .names
            .iter()
            .copied()
            .chain(
                w.fresh
                    .iter()
                    .map(|&range| LifetimeName { name: None, range }),
            )
            .collect();
        let valid = |region| {
            matches!(region, Region::Static)
                || matches!(region, Region::Param(i) if (i as usize) < r.names.len())
        };
        if !w.errors.is_empty()
            || ret != r.ret
            || expected_names.as_slice() != r.names.as_ref()
            || unique.as_slice() != r.outlives.as_ref()
            || r.params
                .iter()
                .flatten()
                .chain(r.ret.iter())
                .any(|&region| !valid(region))
        {
            return Err(fail(TraitProfileReason::SignatureMetadata));
        }
    }
    if items
        .functions
        .iter()
        .any(|(id, f)| f.owner == Some(Owner::Trait(trait_)) && !listed.contains(&id))
    {
        return Err(fail(TraitProfileReason::OwnerMetadata));
    }
    region_free_graph(items, sigs, trait_)?;
    Ok(())
}

/// Validate all represented supertrait edges without recursive graph walks.
fn region_free_graph(
    items: &ItemTree,
    sigs: &Signatures,
    trait_: TraitId,
) -> Result<bool, TraitProfileError> {
    region_graph(items, sigs, trait_, false)
}

fn region_graph(
    items: &ItemTree,
    sigs: &Signatures,
    trait_: TraitId,
    associated: bool,
) -> Result<bool, TraitProfileError> {
    // Iterative DFS keeps even long malformed cycles off the call stack.
    let mut copy = false;
    let mut active = rustc_hash::FxHashSet::default();
    let mut done = rustc_hash::FxHashSet::default();
    let mut work = vec![(trait_, false)];
    while let Some((id, exiting)) = work.pop() {
        if exiting {
            active.remove(&id);
            done.insert(id);
        } else if !done.contains(&id) {
            if !active.insert(id) {
                return Err(TraitProfileError {
                    trait_: id,
                    method: None,
                    reason: TraitProfileReason::SupertraitCycle,
                });
            }
            copy |= items.traits[id].lang == Some(flux_intern::sym::copy);
            let edges = region_free_edges(items, sigs, id, associated && id == trait_)?;
            work.push((id, true));
            work.extend(edges.into_iter().rev().map(|id| (id, false)));
        }
    }
    Ok(copy)
}

/// Check represented safe borrowed or Copy-by-value trait methods, including
/// defaults and method-local type parameters with region-free trait bounds.
/// Trait/method obligations and bodies still need their ordinary checks.
pub fn safe_method_trait_profile(
    hir: &flux_hir::Hir,
    sigs: &Signatures,
    trait_: TraitId,
) -> Result<(), TraitProfileError> {
    safe_method_profile(&hir.items, &hir.diagnostics, sigs, trait_)
}

pub(crate) fn safe_method_profile(
    items: &ItemTree,
    diagnostics: &[flux_hir::Diagnostic],
    sigs: &Signatures,
    trait_: TraitId,
) -> Result<(), TraitProfileError> {
    method_profile(items, diagnostics, sigs, trait_, false)
}

/// Generic methods may name ordinary associated outputs and lifetime-bearing
/// method bounds when their complete occurrence tables can be reconstructed.
pub(crate) fn generic_method_profile(
    items: &ItemTree,
    diagnostics: &[flux_hir::Diagnostic],
    sigs: &Signatures,
    trait_: TraitId,
) -> Result<(), TraitProfileError> {
    method_profile(items, diagnostics, sigs, trait_, true)
}

fn method_profile(
    items: &ItemTree,
    diagnostics: &[flux_hir::Diagnostic],
    sigs: &Signatures,
    trait_: TraitId,
    projected: bool,
) -> Result<(), TraitProfileError> {
    let fail = |method, reason| TraitProfileError {
        trait_,
        method,
        reason,
    };
    let data = items
        .traits
        .get(trait_)
        .ok_or(fail(None, TraitProfileReason::MissingMetadata))?;
    if data.generics.lifetimes.len() != 1 {
        return Err(fail(None, TraitProfileReason::OwnerLifetimes));
    }
    if (!projected && (!data.generics.params.is_empty() || has_predicates(&data.generics)))
        || data.generics.params.iter().any(|p| p.is_const())
    {
        return Err(fail(None, TraitProfileReason::TraitPredicates));
    }
    if (!projected && !data.types.is_empty())
        || !scalar_const_contracts(items, diagnostics, sigs, trait_)
    {
        return Err(fail(None, TraitProfileReason::AssociatedItems));
    }
    if data.is_unsafe {
        return Err(fail(None, TraitProfileReason::UnsafeTrait));
    }
    if diagnosed(diagnostics, data.loc.file, data.def_range) {
        return Err(fail(None, TraitProfileReason::DeclarationDiagnostics));
    }
    let owner = sigs
        .regions
        .traits
        .get(trait_)
        .ok_or(fail(None, TraitProfileReason::MissingMetadata))?;
    if owner.names.as_ref()
        != [LifetimeName {
            name: Some(data.generics.lifetimes[0].name),
            range: data.generics.lifetimes[0].range,
        }]
        || (!projected && !owner.outlives.is_empty())
        || (projected
            && super::resolve::trait_outlives_contract(items, sigs, trait_).as_deref()
                != Some(owner.outlives.as_ref()))
        || (!projected && !owner.families.is_empty())
    {
        return Err(fail(None, TraitProfileReason::OwnerMetadata));
    }
    if projected
        && data
            .types
            .iter()
            .any(|t| !t.generics.lifetimes.is_empty() || !t.generics.params.is_empty())
    {
        return Err(fail(None, TraitProfileReason::AssociatedItems));
    }
    if projected {
        if owner.families.len() != data.types.len() {
            return Err(fail(None, TraitProfileReason::OwnerMetadata));
        }
        let mut expected_projections = Vec::new();
        for family in &data.types {
            if family.bounds.iter().any(|bound| {
                !bound.bound_lifetimes.is_empty()
                    || bound.path.qself.is_some()
                    || bound.path.args.iter().flatten().any(|args| {
                        args.parenthesized
                            || args.types.iter().any(|ty| !written_type(ty, false))
                            || args.bindings.iter().any(|binding| {
                                binding.args.is_some() || !written_type(&binding.ty, false)
                            })
                    })
            }) {
                return Err(fail(None, TraitProfileReason::AssociatedItems));
            }
            let Some((expected, bounds, projections)) =
                super::resolve::ordinary_associated_contract(items, sigs, trait_, family.name)
            else {
                return Err(fail(None, TraitProfileReason::AssociatedItems));
            };
            let saved = owner
                .families
                .iter()
                .find(|(name, _)| *name == family.name)
                .map(|(_, c)| c);
            let erased: Vec<_> = sigs.traits[trait_]
                .type_bounds
                .iter()
                .filter(|(name, _)| *name == family.name)
                .map(|(_, bound)| *bound)
                .collect();
            if saved != Some(&expected) || bounds != erased {
                return Err(fail(None, TraitProfileReason::AssociatedItems));
            }
            expected_projections.extend(projections);
        }
        if expected_projections.as_slice() != sigs.traits[trait_].implied.as_ref()
            || sigs
                .trait_projections
                .get(trait_)
                .map_or(&[][..], |p| p.as_ref())
                != expected_projections.as_slice()
        {
            return Err(fail(None, TraitProfileReason::AssociatedItems));
        }
    }
    let copy = region_graph(items, sigs, trait_, projected)?;
    let sized = sigs.traits.get(trait_).is_some_and(|t| {
        t.supertraits
            .iter()
            .any(|b| Some(b.trait_) == sigs.builtins.sized)
    });
    let mut listed = rustc_hash::FxHashSet::default();
    for &method in &data.functions {
        let bad = |reason| fail(Some(method), reason);
        let function = items
            .functions
            .get(method)
            .ok_or(bad(TraitProfileReason::MissingMetadata))?;
        if !listed.insert(method) || function.owner != Some(Owner::Trait(trait_)) {
            return Err(bad(TraitProfileReason::OwnerMetadata));
        }
        if (!projected && !function.has_self)
            || (!projected && function.is_unsafe)
            || function.is_extern
            || function.is_const
            || function.is_variadic
            || function.intrinsic.is_some()
            || function.derived.is_some()
            || function.ctor.is_some()
            || !function.opaques.is_empty()
        {
            return Err(bad(TraitProfileReason::MethodShape));
        }
        if (!projected
            && (!function.generics.lifetimes.is_empty()
                || !function.generics.outlives.is_empty()
                || !function.generics.type_outlives.is_empty()))
            || function.generics.params.iter().any(|parameter| {
                parameter.is_const()
                    || (!projected && parameter.maybe_unsized)
                    || parameter.default.is_some()
            })
        {
            return Err(bad(TraitProfileReason::MethodGenerics));
        }
        let own_names: Vec<_> = function
            .generics
            .params
            .iter()
            .map(|parameter| parameter.name)
            .collect();
        let synthetic = |index: u32| {
            projected
                && (index as usize)
                    .checked_sub(1 + data.generics.params.len())
                    .and_then(|index| function.generics.params.get(index))
                    .is_some_and(|parameter| parameter.synthetic)
        };
        for predicate in &function.generics.predicates {
            let callback = projected
                && items
                    .resolve_trait(function.loc.module, &predicate.bound)
                    .0
                    .is_some_and(|id| crate::fn_trait_kind(items, id).is_some());
            let subject = match &predicate.ty {
                TypeRef::Path(path) => path.as_name().is_some_and(|name| {
                    own_names.contains(&name)
                        || (projected && data.generics.params.iter().any(|p| p.name == name))
                }),
                TypeRef::ImplParam(index) => synthetic(*index),
                _ => false,
            };
            if !subject
                || predicate.bound.path.qself.is_some()
                || predicate.bound.path.args.iter().flatten().any(|arguments| {
                    (!projected && !arguments.lifetimes.is_empty())
                        || arguments.bindings.iter().any(|binding| {
                            !projected
                                || binding.args.is_some()
                                || !written_type(&binding.ty, false)
                        })
                        || (arguments.parenthesized && !callback)
                        || !arguments.types.iter().all(|ty| written_type(ty, !callback))
                })
                || items
                    .resolve_trait(function.loc.module, &predicate.bound)
                    .0
                    .is_none_or(|id| !projected && !items.traits[id].generics.lifetimes.is_empty())
            {
                return Err(bad(TraitProfileReason::MethodGenerics));
            }
        }
        if !written_type(&function.ret, false)
            || function.params.iter().any(|p| {
                !written_type(&p.ty, false)
                    && !matches!(p.ty,TypeRef::ImplParam(index) if synthetic(index))
            })
        {
            return Err(bad(TraitProfileReason::SignatureType));
        }
        if diagnosed(diagnostics, function.loc.file, function.item_range) {
            return Err(bad(TraitProfileReason::DeclarationDiagnostics));
        }
        let signature = sigs
            .fns
            .get(method)
            .ok_or(bad(TraitProfileReason::MissingMetadata))?;
        let self_ty = Ty::param(0, flux_intern::kw::SelfUpper);
        let receiver = match signature.params.first().copied().map(Ty::kind) {
            Some(TyKind::Param(parameter)) => {
                parameter.index == 0 && (copy || (projected && (sized || !function.has_body)))
            }
            Some(TyKind::Ref(_, pointee)) => pointee == self_ty,
            _ => false,
        };
        if !(receiver || (projected && !function.has_self))
            || signature.params.len() != function.params.len()
            || signature.params.iter().chain([&signature.ret]).any(|&ty| {
                method_slots(
                    items,
                    sigs,
                    ty,
                    1 + data.generics.params.len() + own_names.len(),
                    projected,
                )
                .is_none()
            })
        {
            return Err(bad(TraitProfileReason::SignatureType));
        }
        let regions = sigs
            .regions
            .fns
            .get(method)
            .ok_or(bad(TraitProfileReason::MissingMetadata))?;
        let Some((expected_signature, expected_regions, bounds)) =
            super::resolve::safe_method_contract(items, sigs, method)
        else {
            return Err(bad(TraitProfileReason::SignatureMetadata));
        };
        if !projected
            && bounds.predicates.iter().any(|predicate| {
                type_slots(
                    items,
                    sigs,
                    predicate.self_ty,
                    1 + data.generics.params.len() + own_names.len(),
                ) != Some(0)
                    || predicate.trait_ref.args.iter().any(|&ty| {
                        type_slots(
                            items,
                            sigs,
                            ty,
                            1 + data.generics.params.len() + own_names.len(),
                        ) != Some(0)
                    })
            })
        {
            return Err(bad(TraitProfileReason::MethodGenerics));
        }
        if *signature != expected_signature
            || *regions != expected_regions
            || sigs.fn_predicates.get(method).map_or(&[][..], |p| p) != bounds.predicates.as_slice()
            || sigs.fn_projections.get(method).map_or(&[][..], |p| p)
                != bounds.projections.as_slice()
            || regions.callback_binder == super::CallbackBinder::Unclassified
            || sigs.opaques.get(method).is_some_and(|o| !o.is_empty())
        {
            return Err(bad(TraitProfileReason::SignatureMetadata));
        }
    }
    if items
        .functions
        .iter()
        .any(|(id, f)| f.owner == Some(Owner::Trait(trait_)) && !listed.contains(&id))
    {
        return Err(fail(None, TraitProfileReason::OwnerMetadata));
    }
    Ok(())
}

/// Scalar const values store no lifetime rows. Admission still reconstructs
/// the declaration type and checks its owner and complete empty region table;
/// ordinary const bodies and impl/caller contracts are checked independently.
fn scalar_const_contracts(
    items: &ItemTree,
    diagnostics: &[flux_hir::Diagnostic],
    sigs: &Signatures,
    trait_: TraitId,
) -> bool {
    let names = crate::generic_names(items, Some(Owner::Trait(trait_)), &Generics::default());
    let scope = TypeScope {
        self_ty: Some(Ty::param(0, flux_intern::kw::SelfUpper)),
        generics: &names,
        defaults: Some(&sigs.trait_defaults),
        struct_fields: Some(&sigs.structs),
        ..TypeScope::default()
    };
    items.traits[trait_].consts.iter().all(|&id| {
        let Some(data) = items.consts.get(id) else {
            return false;
        };
        let Some(&ty) = sigs.consts.get(id) else {
            return false;
        };
        if data.owner != Some(Owner::Trait(trait_))
            || data.is_static.is_some()
            || data.anon.is_some()
            || diagnosed(diagnostics, data.loc.file, data.item_range)
            || !matches!(
                ty.kind(),
                TyKind::Bool | TyKind::Char | TyKind::Int(_) | TyKind::Float(_)
            )
            || !sigs
                .regions
                .consts
                .get(id)
                .is_some_and(|row| row.is_empty())
        {
            return false;
        }
        let mut errors = Vec::new();
        crate::resolve_type(
            items,
            data.loc.module,
            &data.ty,
            scope,
            data.ty_range,
            &mut errors,
        ) == ty
            && errors.is_empty()
    })
}

fn method_slots(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    params: usize,
    projected: bool,
) -> Option<usize> {
    if projected && let TyKind::Alias(alias) = ty.kind() {
        for &arg in alias.args.iter().chain(alias.family_args) {
            method_slots(items, sigs, arg, params, projected)?;
        }
        return Some(super::slots(items, ty));
    }
    match ty.kind() {
        TyKind::Ref(_, t) => Some(1 + method_slots(items, sigs, t, params, projected)?),
        TyKind::RawPtr(_, t) | TyKind::Slice(t) | TyKind::Array(t, _) => {
            method_slots(items, sigs, t, params, projected)
        }
        TyKind::Tuple(ts) => ts.iter().try_fold(0, |n, &t| {
            Some(n + method_slots(items, sigs, t, params, projected)?)
        }),
        TyKind::Adt(a, ts) => {
            let r = sigs.regions.structs.get(a.strukt)?;
            if ts.len() != items.structs[a.strukt].generics.params.len() {
                return None;
            }
            ts.iter().try_fold(r.lifetimes as usize, |n, &t| {
                Some(n + method_slots(items, sigs, t, params, projected)?)
            })
        }
        _ => type_slots(items, sigs, ty, params),
    }
}

fn diagnosed(
    diagnostics: &[flux_hir::Diagnostic],
    file: flux_hir::FileId,
    range: flux_syntax::TextRange,
) -> bool {
    diagnostics
        .iter()
        .any(|d| d.file == file && range.start() <= d.range.start() && d.range.end() <= range.end())
}

fn has_predicates(g: &Generics) -> bool {
    !g.predicates.is_empty() || !g.outlives.is_empty() || !g.type_outlives.is_empty()
}

// Intentionally conservative syntax boundary. Fn pointers, opaque types and
// qualified projections have independent binder/selection requirements.
fn written_type(t: &TypeRef, region_free: bool) -> bool {
    match t {
        TypeRef::Path(p) => {
            p.qself.is_none()
                && p.args.iter().flatten().all(|a| {
                    (!region_free || a.lifetimes.is_empty())
                        && a.bindings.is_empty()
                        && !a.parenthesized
                        && a.types.iter().all(|t| written_type(t, region_free))
                })
        }
        TypeRef::Tuple(ts) => ts.iter().all(|t| written_type(t, region_free)),
        TypeRef::Ptr { pointee, .. } => written_type(pointee, region_free),
        TypeRef::Ref { pointee, .. } => !region_free && written_type(pointee, false),
        TypeRef::Array { elem, .. } | TypeRef::Slice(elem) => written_type(elem, region_free),
        TypeRef::Never => true,
        TypeRef::Dyn(object) if !region_free => object.bounds.iter().all(|bound| {
            bound.bound_lifetimes.is_empty()
                && bound.path.args.iter().flatten().all(|args| {
                    !args.parenthesized
                        && args.lifetimes.is_empty()
                        && args.types.iter().all(|ty| written_type(ty, false))
                        && args.bindings.iter().all(|binding| {
                            binding.args.is_none() && written_type(&binding.ty, false)
                        })
                })
        }),
        _ => false,
    }
}

// Safe slot counting: validate ADT metadata before using the ordinary region
// walker, which assumes it was given an intact signature database.
fn type_slots(items: &ItemTree, sigs: &Signatures, t: Ty, params: usize) -> Option<usize> {
    match t.kind() {
        TyKind::Ref(_, t) => Some(1 + type_slots(items, sigs, t, params)?),
        TyKind::RawPtr(_, t) | TyKind::Slice(t) | TyKind::Array(t, _) => {
            type_slots(items, sigs, t, params)
        }
        TyKind::Tuple(ts) => ts
            .iter()
            .try_fold(0, |n, &t| Some(n + type_slots(items, sigs, t, params)?)),
        TyKind::Adt(a, ts) => {
            let data = items.structs.get(a.strukt)?;
            let r = sigs.regions.structs.get(a.strukt)?;
            if ts.len() != data.generics.params.len()
                || r.lifetimes as usize != data.generics.lifetimes.len()
            {
                return None;
            }
            ts.iter().try_fold(r.lifetimes as usize, |n, &t| {
                Some(n + type_slots(items, sigs, t, params)?)
            })
        }
        TyKind::Dynamic(object) => {
            let principal = items.traits.get(object.trait_)?;
            if object.args.len() != principal.generics.params.len()
                || flux_solve::objects::supertraits_of(items, object.trait_)
                    .iter()
                    .any(|&trait_| {
                        !items.traits[trait_].generics.lifetimes.is_empty()
                            || crate::fn_trait_kind(items, trait_).is_some()
                    })
                || object.auto_traits.iter().any(|marker| {
                    items
                        .traits
                        .get(marker.trait_)
                        .is_none_or(|data| marker.name != data.name)
                        || !flux_solve::traits::is_auto_trait(items, marker.trait_)
                        || marker.trait_ == object.trait_
                        || !marker.args.is_empty()
                })
                || object
                    .auto_traits
                    .windows(2)
                    .any(|pair| pair[0].trait_ >= pair[1].trait_)
            {
                return None;
            }
            let associated = flux_solve::objects::dyn_assoc(items, object.trait_);
            if object.assoc_names.len() != associated.len()
                || object.assoc_tys.len() != associated.len()
                || object
                    .assoc_names
                    .iter()
                    .zip(&associated)
                    .any(|(&actual, &(_, expected))| actual != expected)
            {
                return None;
            }
            object
                .args
                .iter()
                .chain(object.assoc_tys)
                .try_fold(1, |count, &arg| {
                    if arg.has_aliases() || arg.has_opaques() || arg.has_infer() || arg.is_dyn() {
                        return None;
                    }
                    Some(count + type_slots(items, sigs, arg, params)?)
                })
        }
        TyKind::Param(p) if (p.index as usize) < params => Some(0),
        TyKind::Int(_)
        | TyKind::Float(_)
        | TyKind::Bool
        | TyKind::Char
        | TyKind::Str
        | TyKind::Never
        | TyKind::CStr
        | TyKind::Value(_) => Some(0),
        _ => None,
    }
}

fn region_free_edges(
    items: &ItemTree,
    sigs: &Signatures,
    id: TraitId,
    associated: bool,
) -> Result<Vec<TraitId>, TraitProfileError> {
    let fail = |reason| TraitProfileError {
        trait_: id,
        method: None,
        reason,
    };
    let t = items
        .traits
        .get(id)
        .ok_or(fail(TraitProfileReason::MissingMetadata))?;
    let sig = sigs
        .traits
        .get(id)
        .ok_or(fail(TraitProfileReason::MissingMetadata))?;
    let regions = sigs
        .regions
        .traits
        .get(id)
        .ok_or(fail(TraitProfileReason::MissingMetadata))?;
    let predicates = sigs
        .trait_predicates
        .get(id)
        .ok_or(fail(TraitProfileReason::MissingMetadata))?;
    // TraitRegions has no predicate/projection region tables. Root type-parameter
    // outlives bounds have their own reconstructible table; other edges must
    // remain region-free. Missing tables cannot stand for represented regions.
    let represented_outlives = associated
        && t.generics.outlives.is_empty()
        && t.generics.type_outlives.iter().all(|bound| {
            matches!(&bound.ty, TypeRef::Path(path) if path.as_name().is_some_and(|name|t.generics.params.iter().any(|p|p.name==name)))
        })
        && super::resolve::trait_outlives_contract(items,sigs,id).as_deref() == Some(regions.outlives.as_ref());
    if !t.generics.outlives.is_empty()
        || (!represented_outlives
            && (!t.generics.type_outlives.is_empty() || !regions.outlives.is_empty()))
        || (!associated && !sig.implied.is_empty())
        || (!associated
            && sigs
                .trait_projections
                .get(id)
                .is_some_and(|p| !p.is_empty()))
    {
        return Err(fail(TraitProfileReason::SupertraitRegions));
    }
    if regions.names.len() != t.generics.lifetimes.len()
        || regions
            .names
            .iter()
            .zip(t.generics.lifetimes.iter())
            .any(|(n, l)| n.name != Some(l.name) || n.range != l.range)
        || sig.defaults.len() != t.generics.params.len()
        || sigs
            .trait_defaults
            .get(id)
            .is_none_or(|d| d.as_ref() != sig.defaults.as_ref())
    {
        return Err(fail(TraitProfileReason::SupertraitMetadata));
    }
    let names = crate::generic_names(items, Some(Owner::Trait(id)), &Generics::default());
    let self_ty = Ty::param(0, flux_intern::kw::SelfUpper);
    let bounds = [flux_solve::traits::trait_self_predicate(items, id)];
    let scope = TypeScope {
        self_ty: Some(self_ty),
        generics: &names,
        bounds: &bounds,
        defaults: Some(&sigs.trait_defaults),
        ..TypeScope::default()
    };
    let valid_bound = |b: &flux_hir::Bound| {
        b.path.qself.is_none()
            && b.path.args.iter().flatten().all(|a| {
                a.lifetimes.is_empty()
                    && a.bindings.is_empty()
                    && !a.parenthesized
                    && a.types.iter().all(|t| written_type(t, true))
            })
    };
    if t.supertraits
        .iter()
        .chain(t.generics.predicates.iter().map(|p| &p.bound))
        .chain(
            t.types
                .iter()
                .filter(|_| !associated)
                .flat_map(|t| t.bounds.iter()),
        )
        .any(|b| !valid_bound(b))
        || t.generics
            .predicates
            .iter()
            .any(|p| !written_type(&p.ty, true))
    {
        return Err(fail(TraitProfileReason::SupertraitRegions));
    }
    let mut errors = Vec::new();
    for (p, actual) in t.generics.params.iter().zip(sig.defaults.iter()) {
        let expected = match &p.default {
            Some((ty, range)) => {
                if !written_type(ty, true) {
                    return Err(fail(TraitProfileReason::SupertraitRegions));
                }
                Some(crate::resolve_type(
                    items,
                    t.loc.module,
                    ty,
                    scope,
                    *range,
                    &mut errors,
                ))
            }
            None => None,
        };
        if expected != *actual {
            return Err(fail(TraitProfileReason::SupertraitMetadata));
        }
        if actual.is_some_and(|ty| type_slots(items, sigs, ty, names.len()) != Some(0)) {
            return Err(fail(TraitProfileReason::SupertraitRegions));
        }
    }
    let mut supers = Vec::new();
    for b in &t.supertraits {
        if let Some((edge, _)) =
            crate::traits::resolve_bound(items, t.loc.module, b, self_ty, scope, &mut errors)
        {
            supers.push(edge);
        }
    }
    let resolved =
        crate::resolve_predicates(items, sigs, t.loc.module, &t.generics, scope, &mut errors);
    let mut own = Vec::new();
    for p in resolved.predicates {
        if p.self_ty == self_ty {
            supers.push(p.trait_ref);
        } else {
            own.push(p);
        }
    }
    // Independently verify associated-bound metadata without imposing the root
    // profile's ban on associated items on a general region-free supertrait.
    let mut type_bounds = Vec::new();
    for ty in &t.types {
        let alias = Ty::alias(flux_solve::traits::alias_of(
            items,
            bounds[0].trait_ref,
            self_ty,
            ty.name,
        ));
        for b in &ty.bounds {
            if let Some((edge, _)) =
                crate::traits::resolve_bound(items, t.loc.module, b, alias, scope, &mut errors)
            {
                type_bounds.push((ty.name, edge));
            }
        }
    }
    if !errors.is_empty()
        || supers.as_slice() != sig.supertraits.as_ref()
        || own.as_slice() != predicates.as_ref()
        || !resolved.projections.is_empty()
        || type_bounds.as_slice() != sig.type_bounds.as_ref()
    {
        return Err(fail(TraitProfileReason::SupertraitMetadata));
    }
    for p in predicates.iter() {
        if type_slots(items, sigs, p.self_ty, names.len()) != Some(0) {
            return Err(fail(TraitProfileReason::SupertraitRegions));
        }
    }
    for edge in sig
        .supertraits
        .iter()
        .chain(predicates.iter().map(|p| &p.trait_ref))
        .chain(
            sig.type_bounds
                .iter()
                .filter(|_| !associated)
                .map(|(_, r)| r),
        )
    {
        let target = items
            .traits
            .get(edge.trait_)
            .ok_or(fail(TraitProfileReason::MissingMetadata))?;
        if !target.generics.lifetimes.is_empty()
            || edge.args.len() != target.generics.params.len()
            || edge
                .args
                .iter()
                .any(|&t| type_slots(items, sigs, t, names.len()) != Some(0))
        {
            return Err(fail(TraitProfileReason::SupertraitRegions));
        }
        // Predicate targets also need intact region metadata, but are not
        // supertrait edges (recursive generic bounds need not form a cycle).
        let r = sigs
            .regions
            .traits
            .get(edge.trait_)
            .ok_or(fail(TraitProfileReason::MissingMetadata))?;
        if !r.names.is_empty() {
            return Err(fail(TraitProfileReason::SupertraitMetadata));
        }
        if !r.outlives.is_empty() {
            return Err(fail(TraitProfileReason::SupertraitRegions));
        }
    }
    Ok(sig.supertraits.iter().map(|e| e.trait_).collect())
}

/// Associated-family methods have explicit owner and local binders. This
/// profile admits their represented region rows, independently of the older
/// required-method profile; proving their contracts remains mandatory in MIR.
pub(crate) fn family_method_profile(
    items: &ItemTree,
    diagnostics: &[flux_hir::Diagnostic],
    sigs: &Signatures,
    id: TraitId,
) -> bool {
    let Some(data) = items.traits.get(id) else {
        return false;
    };
    if data.generics.lifetimes.len() != 1
        || !data.generics.params.is_empty()
        || !data.generics.predicates.is_empty()
        || !data.consts.is_empty()
        || data.types.is_empty()
        || data.is_unsafe
    {
        return false;
    }
    let Some(owner) = sigs.regions.traits.get(id) else {
        return false;
    };
    if owner.names.as_ref()
        != [LifetimeName {
            name: Some(data.generics.lifetimes[0].name),
            range: data.generics.lifetimes[0].range,
        }]
        || owner.families.len() != data.types.len()
        || diagnosed(diagnostics, data.loc.file, data.def_range)
    {
        return false;
    }
    for family in &data.types {
        let Some((_, contract)) = owner.families.iter().find(|(name, _)| *name == family.name)
        else {
            return false;
        };
        if contract.owner != 1
            || contract.names.len() != 1 + family.generics.lifetimes.len()
            || !family.generics.params.is_empty()
            || !family.generics.predicates.is_empty()
        {
            return false;
        }
        if contract.names.first() != owner.names.first()
            || contract
                .names
                .iter()
                .skip(1)
                .zip(&family.generics.lifetimes)
                .any(|(n, l)| n.name != Some(l.name) || n.range != l.range)
        {
            return false;
        }
        let implicit_sized = sigs.builtins.sized.filter(|sized| {
            !family.maybe_unsized
                && (!family.generics.lifetimes.is_empty() || !family.generics.params.is_empty())
                && !sigs.traits.get(id).is_some_and(|signature| {
                    signature
                        .type_bounds
                        .iter()
                        .any(|(name, bound)| *name == family.name && bound.trait_ == *sized)
                })
        });
        if contract.output_traits.len()
            != family.bounds.len() + usize::from(implicit_sized.is_some())
        {
            return false;
        }
        if let Some(sized) = implicit_sized
            && !contract
                .output_traits
                .last()
                .is_some_and(|(bound, regions)| {
                    bound.trait_ == sized
                        && bound.name == items.traits[sized].name
                        && bound.args.is_empty()
                        && *regions == super::TraitRefRegions::default()
                })
        {
            return false;
        }
    }
    fn ty_ok(ty: Ty) -> bool {
        match ty.kind() {
            TyKind::FnPtr(_)
            | TyKind::Dynamic(_)
            | TyKind::Opaque(_)
            | TyKind::Infer(_)
            | TyKind::Error => false,
            _ => ty.children().all(ty_ok),
        }
    }
    for &method in &data.functions {
        let function = &items.functions[method];
        let (Some(signature), Some(regions)) = (sigs.fns.get(method), sigs.regions.fns.get(method))
        else {
            return false;
        };
        if function.owner != Some(Owner::Trait(id))
            || function.is_unsafe
            || function.is_extern
            || function.is_const
            || function.is_variadic
            || !function.generics.params.is_empty()
            || !function.generics.predicates.is_empty()
            || !function.opaques.is_empty()
            || regions.owner != 1
            || regions.names.first() != owner.names.first()
            || diagnosed(diagnostics, function.loc.file, function.item_range)
        {
            return false;
        }
        if !ty_ok(signature.ret)
            || signature.params.iter().any(|&ty| !ty_ok(ty))
            || regions.params.len() != signature.params.len()
        {
            return false;
        }
        let valid = |r: &Region| {
            matches!(r, Region::Static)
                || matches!(r,Region::Param(i)if(*i as usize)<regions.names.len())
        };
        if regions.ret.len() != super::slots(items, signature.ret)
            || !regions.ret.iter().all(valid)
            || signature
                .params
                .iter()
                .zip(regions.params.iter())
                .any(|(&ty, row)| row.len() != super::slots(items, ty) || !row.iter().all(valid))
        {
            return false;
        }
        let names = crate::generic_names(items, Some(Owner::Trait(id)), &function.generics);
        let mut walker = Walker::new(
            items,
            function.loc.module,
            owner
                .names
                .iter()
                .map(|n| n.name)
                .chain(function.generics.lifetimes.iter().map(|l| Some(l.name)))
                .collect(),
        );
        walker.scope = TypeScope {
            self_ty: Some(Ty::param(0, flux_intern::kw::SelfUpper)),
            generics: &names,
            ..TypeScope::default()
        };
        walker.self_projection_owner = Some((id, Box::new([Region::Param(0)])));
        walker.elided = Elided::Fresh;
        let mut distinct = Vec::new();
        for ((written, &ty), expected) in function
            .params
            .iter()
            .zip(signature.params.iter())
            .zip(regions.params.iter())
        {
            let row = walker.regions(&written.ty, ty);
            if row != *expected {
                return false;
            }
            let mut own = Vec::new();
            for &r in &row {
                if !own.contains(&r) {
                    own.push(r)
                }
            }
            distinct.extend(own);
        }
        walker.elided = if function.has_self
            && matches!(
                function.params.first().map(|p| &p.ty),
                Some(TypeRef::Ref { .. })
            ) {
            Elided::Is(
                regions
                    .params
                    .first()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(Region::Erased),
            )
        } else if distinct.len() == 1 {
            Elided::Is(distinct[0])
        } else {
            Elided::Missing(None)
        };
        if walker.regions(&function.ret, signature.ret) != regions.ret || !walker.errors.is_empty()
        {
            return false;
        }
        let expected_names: Vec<_> = owner
            .names
            .iter()
            .copied()
            .chain(function.generics.lifetimes.iter().map(|l| LifetimeName {
                name: Some(l.name),
                range: l.range,
            }))
            .chain(
                walker
                    .fresh
                    .iter()
                    .map(|&range| LifetimeName { name: None, range }),
            )
            .collect();
        if expected_names.as_slice() != regions.names.as_ref() {
            return false;
        }
    }
    data.supertraits.iter().all(|bound| {
        items
            .resolve_trait(data.loc.module, bound)
            .0
            .is_some_and(|t| items.traits[t].generics.lifetimes.is_empty())
    })
}

/// Marker declarations have no method, associated-value, or stored-field
/// contracts. Their lifetime parameters remain independent trait arguments.
pub(crate) fn lifetime_marker_profile(
    items: &ItemTree,
    diagnostics: &[flux_hir::Diagnostic],
    sigs: &Signatures,
    trait_: TraitId,
) -> bool {
    let data = &items.traits[trait_];
    let Some(regions) = sigs.regions.traits.get(trait_) else {
        return false;
    };
    !data.is_unsafe
        && data.functions.is_empty()
        && data.types.is_empty()
        && data.consts.is_empty()
        && data.supertraits.is_empty()
        && data.generics.params.is_empty()
        && !has_predicates(&data.generics)
        && !diagnosed(diagnostics, data.loc.file, data.def_range)
        && regions.names.len() == data.generics.lifetimes.len()
        && regions
            .names
            .iter()
            .zip(&data.generics.lifetimes)
            .all(|(resolved, written)| {
                resolved.name == Some(written.name) && resolved.range == written.range
            })
        && regions.outlives.is_empty()
        && regions.families.is_empty()
        && regions.supertraits.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_associated_bounds_reconstruct_contracts() {
        let source = include_str!("../../../../tests/run/serde_associated_lifetimes.rs");
        let (_, file, hir) = flux_hir::lower_text("output-profile.rs", source);
        let (sigs, _) = crate::signatures(&hir.items);
        for (owner, data) in hir
            .items
            .traits
            .iter()
            .filter(|(_, t)| t.loc.file == file && t.name.as_str() == "Access")
        {
            let result = generic_method_profile(&hir.items, &hir.diagnostics, &sigs, owner);
            assert!(
                result.is_ok(),
                "{}: {result:?}; predicates={:?}; projections={:?}; outlives={:?}",
                data.name,
                sigs.trait_predicates.get(owner),
                sigs.trait_projections.get(owner),
                sigs.regions.traits[owner].outlives
            );
        }
    }

    #[test]
    fn associated_output_metadata_corruption_is_rejected() {
        let source = include_str!("../../../../tests/run/serde_associated_lifetimes.rs");
        let (_, file, hir) = flux_hir::lower_text("output-corruption.rs", source);
        let (sigs, errors) = crate::signatures(&hir.items);
        assert!(errors.is_empty(), "{errors:?}");
        let owner = hir
            .items
            .traits
            .iter()
            .find(|(_, t)| t.loc.file == file && t.name.as_str() == "Access")
            .unwrap()
            .0;
        for corruption in 0..6 {
            let mut bad = sigs.clone();
            let mut row = bad.regions.traits[owner].clone();
            let contract = &mut row
                .families
                .iter_mut()
                .find(|(name, _)| name.as_str() == "Variant")
                .unwrap()
                .1;
            match corruption {
                0 => contract.output_traits[0].1.lifetimes[0] = Region::Static,
                1 => contract.output_projections = Box::default(),
                2 => contract.owner = 0,
                3 => contract.names = Box::default(),
                4 => {
                    let mut signature = bad.traits[owner].clone();
                    signature.implied = Box::default();
                    bad.traits.insert(owner, signature);
                }
                _ => {
                    bad.trait_projections.insert(owner, Box::default());
                }
            }
            bad.regions.traits.insert(owner, row);
            assert!(
                generic_method_profile(&hir.items, &[], &bad, owner).is_err(),
                "corruption {corruption}"
            );
        }
    }

    #[test]
    fn trait_owned_argument_metadata_is_checked() {
        let source = include_str!("../../../../tests/run/trait_argument_bounds.rs");
        let (_, file, hir) = flux_hir::lower_text("owner-profile.rs", source);
        let (sigs, errors) = crate::signatures(&hir.items);
        assert!(errors.is_empty(), "{errors:?}");
        let owner = hir
            .items
            .traits
            .iter()
            .find(|(_, t)| t.loc.file == file && t.name.as_str() == "IntoReader")
            .unwrap()
            .0;
        assert!(generic_method_profile(&hir.items, &[], &sigs, owner).is_ok());
        for corruption in 0..4 {
            let mut bad = sigs.clone();
            match corruption {
                0 => {
                    bad.trait_predicates.insert(owner, Box::default());
                }
                1 => {
                    bad.trait_defaults.insert(owner, Box::default());
                }
                2 => {
                    let mut signature = bad.traits[owner].clone();
                    signature.families[0].1.owner_params = 1;
                    bad.traits.insert(owner, signature);
                }
                _ => {
                    let mut row = bad.regions.traits[owner].clone();
                    row.families[0].1.output_projections[0].0.ty = Ty::BOOL;
                    bad.regions.traits.insert(owner, row);
                }
            }
            assert!(
                generic_method_profile(&hir.items, &[], &bad, owner).is_err(),
                "corruption {corruption}"
            );
        }
    }

    #[test]
    fn generic_method_profile_retains_owner_type_outlives() {
        let source = "trait Keys<'a,K:'a>:Copy{fn len(self)->usize;unsafe fn get(self,index:usize)->&'a K;fn slice(self)->Self;fn find<Q:Ord+?Sized>(self,key:&Q)->usize where K:core::borrow::Borrow<Q>{0}fn search(self,compare:impl Fn(&K)->core::cmp::Ordering)->usize{0}}";
        let (_, file, hir) = flux_hir::lower_text("keys-profile.rs", source);
        let (sigs, diagnostics) = crate::signatures(&hir.items);
        let (owner, _) = hir
            .items
            .traits
            .iter()
            .find(|(_, t)| t.loc.file == file)
            .unwrap();
        assert_eq!(
            generic_method_profile(&hir.items, &[], &sigs, owner),
            Ok(())
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let mut bad = sigs.clone();
        bad.regions.traits.get_mut(owner).unwrap().outlives = Box::default();
        assert!(generic_method_profile(&hir.items, &[], &bad, owner).is_err());
        let callback = *hir.items.traits[owner].functions.last().unwrap();
        let mut bad = sigs.clone();
        bad.regions.fns.get_mut(callback).unwrap().bound_traits = Box::default();
        assert!(generic_method_profile(&hir.items, &[], &bad, owner).is_err());
    }

    #[test]
    fn generic_method_profile_reconstructs_contracts() {
        let source = "trait Decode<'de>:Sized{type Output;fn decode<'s,D:Read<'de>>(&'s self,d:D)->Result<Self::Output,D::Error>;}trait Read<'de>:Sized{type Error;fn bytes(self)->&'de str;}";
        let (_, file, hir) = flux_hir::lower_text("generic-profile.rs", source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let (sigs, diagnostics) = crate::signatures(&hir.items);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let (owner, declaration) = hir
            .items
            .traits
            .iter()
            .find(|(_, t)| t.loc.file == file && t.name.as_str() == "Decode")
            .unwrap();
        let method = declaration.functions[0];
        assert!(generic_method_profile(&hir.items, &[], &sigs, owner).is_ok());
        for corruption in 0..13 {
            let mut bad = sigs.clone();
            let mut row = bad.regions.fns[method].clone();
            match corruption {
                0 => row.owner = 0,
                1 => row.names = Box::default(),
                2 => row.params = Box::default(),
                3 => row.params[0][0] = Region::Static,
                4 => row.bound_traits = Box::default(),
                5 => row.bound_selfs = Box::default(),
                6 => row.callback_binder = super::super::CallbackBinder::Unclassified,
                7 => {
                    bad.fn_predicates.insert(method, Box::default());
                }
                8 => {
                    let mut owner_row = bad.regions.traits[owner].clone();
                    owner_row.families = Box::default();
                    bad.regions.traits.insert(owner, owner_row);
                }
                9 => row.selected_inputs = Box::default(),
                10 => *row.selected_inputs.last_mut().unwrap() = Box::default(),
                11 => row.selected_inputs.last_mut().unwrap()[0].lifetimes[0] = Region::Static,
                _ => {
                    let mut signature = bad.fns[method].clone();
                    signature.ret = Ty::BOOL;
                    bad.fns.insert(method, signature);
                }
            }
            bad.regions.fns.insert(method, row);
            assert!(
                generic_method_profile(&hir.items, &[], &bad, owner).is_err(),
                "corruption {corruption}"
            );
        }
    }
}
