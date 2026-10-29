//! Trait objects: `dyn Tr` as a type.
//!
//! A trait object's type ([`TyKind::Dynamic`]) is its trait, the
//! trait's generic arguments, and every associated type of the trait
//! and its supertraits, which must all be given (`dyn Iterator<Item =
//! u32>`). It implements its trait and their supertraits. Its lifetime
//! bound (`dyn Tr + 'a`) is a region slot, kept beside the type as
//! lifetimes are ([`crate::regions`]).
//!
//! What makes a trait usable as one (rustc's *dyn compatibility*), and
//! the supertraits and associated types an object of it has, are read
//! from the item tree alone by `flux_solve::objects`.

use flux_hir::{DynRef, ItemTree, ModuleId, TraitId};
use flux_intern::kw;
use flux_syntax::TextRange;

use crate::ty::{DynTy, symbol_list};
use crate::{Ty, TyKind, TypeScope};

/// The trait object behind a reference, raw pointer or owning Box target.
/// Its default methods must be collected even when no direct call names them.
pub fn unsize_trait(items: &ItemTree, target: Ty) -> Option<TraitId> {
    let pointee = match target.kind() {
        TyKind::Ref(_, pointee) | TyKind::RawPtr(_, pointee) => pointee,
        _ => flux_solve::box_pointee(target).or_else(|| crate::arc_pointee(items, target))?,
    };
    match pointee.kind() {
        TyKind::Dynamic(object) => Some(object.trait_),
        _ => None,
    }
}

/// The `Self` a trait object's bounds are resolved with: no type, so
/// that a default naming `Self` shows (E0393).
fn object_self() -> Ty {
    Ty::param(u32::MAX, kw::SelfUpper)
}

/// `core::ptr::drop_in_place`, if `core` has it: what a vtable's drop
/// function calls.
pub fn drop_in_place(items: &ItemTree) -> Option<flux_hir::FunctionId> {
    items.functions.iter().find_map(|(id, f)| {
        let in_ptr =
            items.is_core(f.loc.module) && items.modules[f.loc.module].name.as_str() == "ptr";
        (in_ptr && f.owner.is_none() && f.name.as_str() == "drop_in_place" && f.has_body)
            .then_some(id)
    })
}

/// The type `dyn Tr<..> + 'a` stands for, written in `module`: its
/// trait must be dyn compatible (E0038), give every associated type
/// (E0191), and be the only trait (E0225; flux has no auto traits).
pub(crate) fn resolve_dyn(
    items: &ItemTree,
    module: ModuleId,
    d: &DynRef,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let fail = |errors: &mut Vec<(TextRange, String)>, at: TextRange, message: String| {
        errors.push((at, message));
        Ty::ERROR
    };
    let mut resolved = Vec::new();
    for bound in &d.bounds {
        let (trait_, error) = items.resolve_trait(module, bound);
        if let Some(error) = error {
            errors.push(error);
        }
        let Some(trait_) = trait_ else {
            return Ty::ERROR;
        };
        resolved.push((bound, trait_));
    }
    // The principal can be written after marker bounds. Pure auto-trait
    // objects choose a canonical principal independent of source ordering.
    let (first, t) = resolved
        .iter()
        .copied()
        .find(|&(_, t)| !flux_solve::traits::is_auto_trait(items, t))
        .or_else(|| resolved.iter().copied().min_by_key(|&(_, t)| t))
        .expect("nonempty dyn bounds");
    let mut auto_traits = Vec::new();
    for &(extra, marker) in &resolved {
        if std::ptr::eq(first, extra)
            || marker == t && flux_solve::traits::is_auto_trait(items, marker)
        {
            continue;
        }
        if !flux_solve::traits::is_auto_trait(items, marker) {
            return fail(
                errors,
                extra.range,
                "only auto traits can be used as additional traits in a trait object".into(),
            );
        }
        if extra
            .path
            .args(extra.path.segments.len().saturating_sub(1))
            .is_some()
        {
            return fail(
                errors,
                extra.range,
                "auto traits do not take generic arguments".into(),
            );
        }
        let bound = crate::TraitRef {
            trait_: marker,
            name: items.traits[marker].name,
            args: Ty::list(&[]),
        };
        if !auto_traits.contains(&bound) {
            auto_traits.push(bound);
        }
    }
    auto_traits.sort_by_key(|bound| bound.trait_);
    let path = &first.path;
    let last = path.segments.len().saturating_sub(1);
    let name_range = path.range(last, first.range);
    // The path and its arguments, where rustc reports what they lack.
    let path_range = match (path.ranges.first(), path.args(last)) {
        (Some(start), Some(args)) => TextRange::new(start.start(), args.range.end()),
        (Some(start), None) => TextRange::new(start.start(), name_range.end()),
        _ => first.range,
    };
    let self_ty = object_self();
    let Some((trait_ref, projections)) =
        crate::traits::trait_ref(items, module, t, first, self_ty, scope, errors)
    else {
        return Ty::ERROR;
    };
    let data = &items.traits[t];
    for (i, &arg) in trait_ref.args.iter().enumerate() {
        if crate::mentions_param(arg, u32::MAX) {
            let param = data.generics.params[i].name;
            return fail(
                errors,
                name_range,
                format!("the type parameter `{param}` must be explicitly specified"),
            );
        }
    }
    if let Some(reason) = flux_solve::objects::dyn_incompatibility(items, t) {
        // rustc points at the trait for its supertraits' `Self`.
        let at = if reason.contains("as a type parameter") {
            name_range
        } else {
            d.range
        };
        return fail(
            errors,
            at,
            format!("the trait `{}` is not dyn compatible", data.name),
        );
    }
    let assoc = flux_solve::objects::dyn_assoc(items, t);
    let mut names = Vec::new();
    let mut tys = Vec::new();
    let mut missing = Vec::new();
    for (owner, name) in assoc {
        if names.contains(&name) {
            return fail(
                errors,
                d.range,
                "trait objects of traits with two associated types of one name are not supported yet"
                    .into(),
            );
        }
        let given = projections
            .iter()
            .find(|p| p.alias.name == name && p.alias.trait_ == owner)
            .or_else(|| projections.iter().find(|p| p.alias.name == name));
        match given {
            Some(p) => {
                names.push(name);
                tys.push(p.ty);
            }
            None => missing.push(name),
        }
    }
    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(|n| format!("`{n}`")).collect();
        let (kind, list) = match list.as_slice() {
            [one] => ("type", one.clone()),
            [init @ .., last] => ("types", format!("{} and {last}", init.join(", "))),
            [] => unreachable!(),
        };
        let at = if path.args(last).is_some() {
            path_range
        } else {
            name_range
        };
        return fail(
            errors,
            at,
            format!(
                "the value of the associated {kind} {list} in `{}` must be specified",
                data.name
            ),
        );
    }
    if tys.contains(&Ty::ERROR) {
        return Ty::ERROR;
    }
    // A callable with only bound input regions stores none of those regions
    // in the object. Resolve the written input tuple to distinguish genuinely
    // elided regions from free/static regions hidden behind aliases.
    let callable_inputs = crate::fn_trait_kind(items, t).is_some()
        && tys
            .iter()
            .all(|&ty| crate::regions::slots(items, ty) == 0 && !mentions_object(ty))
        && path.args(last).is_some_and(|written| {
            if !written.parenthesized {
                return false;
            }
            let ([input], [tuple]) = (&*written.types, trait_ref.args) else {
                return false;
            };
            if tuple.has_params() || mentions_object(*tuple) {
                return false;
            }
            let mut walker = crate::regions::Walker::new(items, module, Vec::new());
            walker.scope = scope;
            walker.elided = crate::regions::Elided::Fresh;
            let regions = walker.regions(input, *tuple);
            walker.errors.is_empty()
                && regions.iter().enumerate().all(|(i, r)| {
                    // DynTy currently has no table for repeated/free input
                    // regions. Admit independent binders only, rather than lose
                    // correlations introduced by a lifetime-bearing type alias.
                    matches!(r, crate::regions::Region::Param(_)) && !regions[..i].contains(r)
                })
        });
    // Non-callable object arguments have explicit occurrence rows. Nested
    // objects, pointers with binders, and projected/opaque arguments need
    // independent schemas and retain their existing boundary.
    fn represented_argument(ty: Ty) -> bool {
        !matches!(
            ty.kind(),
            TyKind::Dynamic(_) | TyKind::FnPtr(_) | TyKind::Alias(_) | TyKind::Opaque(_)
        ) && ty.children().all(represented_argument)
    }
    let represented = crate::fn_trait_kind(items, t).is_none()
        && trait_ref
            .args
            .iter()
            .chain(&tys)
            .all(|&ty| represented_argument(ty));
    if trait_ref
        .args
        .iter()
        .chain(&tys)
        .any(|&ty| crate::regions::slots(items, ty) > 0 || mentions_object(ty))
        && !callable_inputs
        && !represented
    {
        return fail(
            errors,
            range,
            "references and trait objects in trait objects' arguments are not supported yet".into(),
        );
    }
    Ty::new(TyKind::Dynamic(DynTy {
        auto_traits: crate::ty::object_auto_traits(&auto_traits),
        trait_: t,
        name: data.name,
        args: trait_ref.args,
        assoc_names: symbol_list(&names),
        assoc_tys: Ty::list(&tys),
    }))
}

/// Whether `ty` contains a trait object.
fn mentions_object(ty: Ty) -> bool {
    ty.is_dyn() || ty.children().any(mentions_object)
}

/// Object metadata can be reused when only auto-trait bounds are erased.
pub fn same_principal(a: Ty, b: Ty) -> bool {
    matches!((a.kind(), b.kind()), (TyKind::Dynamic(x), TyKind::Dynamic(y))
        if x.trait_ == y.trait_ && x.args == y.args && x.assoc_names == y.assoc_names && x.assoc_tys == y.assoc_tys)
}

/// Explicit and inherited marker bounds survive an object upcast.
pub fn retains_auto_traits(sigs: &crate::Signatures, source: Ty, target: Ty) -> bool {
    let (TyKind::Dynamic(from), TyKind::Dynamic(to)) = (source.kind(), target.kind()) else {
        return false;
    };
    let bounds = flux_solve::traits::dyn_bounds(sigs, source, from);
    to.auto_traits
        .iter()
        .all(|marker| bounds.iter().any(|bound| bound.trait_ref == *marker))
}

/// Marker-only objects have no method slots beyond the common vtable header.
pub fn is_auto_object(items: &ItemTree, ty: Ty) -> bool {
    matches!(ty.kind(), TyKind::Dynamic(object) if flux_solve::traits::is_auto_trait(items, object.trait_))
}
