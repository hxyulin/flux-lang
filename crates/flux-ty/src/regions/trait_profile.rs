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
/// Signatures conservatively exclude function pointers, dyn/opaque types and
/// qualified or unresolved associated projections.
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
            let edges = region_free_edges(items, sigs, id)?;
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
    if !data.generics.params.is_empty() || has_predicates(&data.generics) {
        return Err(fail(None, TraitProfileReason::TraitPredicates));
    }
    if !data.types.is_empty() || !data.consts.is_empty() {
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
        || !owner.outlives.is_empty()
        || !owner.families.is_empty()
    {
        return Err(fail(None, TraitProfileReason::OwnerMetadata));
    }
    let copy = region_free_graph(items, sigs, trait_)?;
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
        if !function.has_self
            || function.is_unsafe
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
        if !function.generics.lifetimes.is_empty()
            || !function.generics.outlives.is_empty()
            || !function.generics.type_outlives.is_empty()
            || function.generics.params.iter().any(|parameter| {
                parameter.is_const() || parameter.maybe_unsized || parameter.default.is_some()
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
        for predicate in &function.generics.predicates {
            if !matches!(&predicate.ty,TypeRef::Path(path) if path.as_name().is_some_and(|name|own_names.contains(&name)))
                || predicate.bound.path.qself.is_some()
                || predicate.bound.path.args.iter().flatten().any(|arguments| {
                    !arguments.lifetimes.is_empty()
                        || !arguments.bindings.is_empty()
                        || arguments.parenthesized
                        || !arguments.types.iter().all(|ty| written_type(ty, true))
                })
                || items
                    .resolve_trait(function.loc.module, &predicate.bound)
                    .0
                    .is_none_or(|id| !items.traits[id].generics.lifetimes.is_empty())
            {
                return Err(bad(TraitProfileReason::MethodGenerics));
            }
        }
        if !written_type(&function.ret, false)
            || function.params.iter().any(|p| !written_type(&p.ty, false))
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
            Some(TyKind::Param(parameter)) => parameter.index == 0 && copy,
            Some(TyKind::Ref(_, pointee)) => pointee == self_ty,
            _ => false,
        };
        if !receiver
            || signature.params.len() != function.params.len()
            || signature
                .params
                .iter()
                .chain([&signature.ret])
                .any(|&ty| type_slots(items, sigs, ty, 1 + own_names.len()).is_none())
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
        if bounds.predicates.iter().any(|predicate| {
            type_slots(items, sigs, predicate.self_ty, 1 + own_names.len()) != Some(0)
                || predicate
                    .trait_ref
                    .args
                    .iter()
                    .any(|&ty| type_slots(items, sigs, ty, 1 + own_names.len()) != Some(0))
        }) {
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

// Intentionally conservative syntax boundary. Fn pointers, dyn/opaque types and
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
    // TraitRegions has no predicate/projection region tables. Check that none
    // are needed; absent tables cannot stand for unrepresented regions.
    if !t.generics.outlives.is_empty()
        || !t.generics.type_outlives.is_empty()
        || !regions.outlives.is_empty()
        || !sig.implied.is_empty()
        || sigs
            .trait_projections
            .get(id)
            .is_some_and(|p| !p.is_empty())
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
    let bounds = [crate::traits::trait_self_predicate(items, id)];
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
        .chain(t.types.iter().flat_map(|t| t.bounds.iter()))
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
        let alias = Ty::alias(crate::traits::alias_of(
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
        .chain(sig.type_bounds.iter().map(|(_, r)| r))
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
            (!family.generics.lifetimes.is_empty() || !family.generics.params.is_empty())
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
