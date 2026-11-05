//! Trait solving, and the queries over [`Signatures`] and the item tree
//! that the solver and the type checker share: whether a predicate holds
//! outside of bodies and what associated types normalize to
//! ([`traits`]), the types of what values hold ([`fields`]), and whether
//! a type may be unsized ([`maybe_unsized`]).
//!
//! Of the type checker's crates it depends on `flux-ty-ir` alone, so it
//! cannot reach into inference.

pub mod fn_regions;
pub mod objects;
mod struct_fields;
pub mod traits;
pub mod type_identity;

pub use struct_fields::{
    FieldTypesScope, array_unsize, contents, field_types, fields, remember_field_types,
    variant_fields, variants,
};

use flux_arena::LayeredArenaMap;
use flux_hir::{Generics, ItemTree, Owner, Struct, StructId, TypeRef};
use flux_intern::{Symbol, kw, sym};
use flux_ty_ir::{
    ClosureArgs, ClosureKind, ConstInstance, DynTy, FnPtrRegions, FnRegion, FnSig, FnSigTy,
    Instance, Mutability, OpaqueSig, Predicate, Signatures, Ty, TyKind, fn_trait_kind, impl_index,
    max_param, mentions_param, metadata, params_of, regions, ty,
};

/// Which generic parameters of an item in `owner` with its own
/// `generics` have no implicit `Sized` bound, in the order of
/// `flux_ty::generic_names`: a trait's `Self`, and those bounded `?Sized`.
pub fn unsized_params(items: &ItemTree, owner: Option<Owner>, own: &Generics) -> Vec<bool> {
    let mut out = Vec::new();
    if let Some(Owner::Trait(_)) = owner {
        out.push(true);
    }
    out.extend(
        items
            .owner_generics(owner)
            .into_iter()
            .chain([own])
            .flat_map(|g| g.params.iter().map(|p| p.maybe_unsized)),
    );
    out
}

/// Whether values of `ty` may have no size known at compile time: a
/// slice, `str`, or a generic parameter without an implicit `Sized`
/// bound (`unsized_params`, by index) that none of `bounds` says is
/// `Sized`.
pub fn maybe_unsized(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    unsized_params: &[bool],
    bounds: &[Predicate],
) -> bool {
    maybe_unsized_with(items, Some(&sigs.structs), ty, unsized_params, bounds)
}

pub fn maybe_unsized_with(
    items: &ItemTree,
    fields: Option<&LayeredArenaMap<Struct, &'static [Ty]>>,
    ty: Ty,
    unsized_params: &[bool],
    bounds: &[Predicate],
) -> bool {
    let ty = metadata::tail(fields, ty, |id| items.structs[id].is_enum());
    match ty.kind() {
        TyKind::Alias(alias) => {
            items.traits[alias.trait_]
                .types
                .iter()
                .any(|t| t.name == alias.name && t.maybe_unsized)
                && !bounds
                    .iter()
                    .any(|b| b.self_ty == ty && trait_requires_sized(items, b.trait_ref.trait_))
        }
        TyKind::Param(p) => {
            unsized_params.get(p.index as usize) == Some(&true)
                && !bounds
                    .iter()
                    .any(|b| b.self_ty == ty && trait_requires_sized(items, b.trait_ref.trait_))
        }
        _ => ty.is_unsized(),
    }
}

/// A supertrait's Self bound applies even before its signature is installed.
fn trait_requires_sized(items: &ItemTree, root: flux_hir::TraitId) -> bool {
    let mut work = vec![root];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(id) = work.pop() {
        if !seen.insert(id) {
            continue;
        }
        let data = &items.traits[id];
        if data.lang == Some(sym::sized) {
            return true;
        }
        for bound in data.supertraits.iter().chain(data.generics.predicates.iter().filter(|p| matches!(&p.ty, TypeRef::Path(path) if path.as_name() == Some(kw::SelfUpper))).map(|p| &p.bound)) {
            if let (Some(target), _) = items.resolve_trait(data.loc.module, bound) { work.push(target); }
        }
    }
    false
}

/// What `ty` holds if it is `alloc`'s `Box<T>` (`#[lang = "owned_box"]`),
/// which dereferences as a built-in pointer that owns its value does, as
/// rustc's: moves out of `*b`, and `*b` is mutable where `b` is.
pub fn box_pointee(ty: Ty) -> Option<Ty> {
    static OWNED_BOX: std::sync::OnceLock<Option<StructId>> = std::sync::OnceLock::new();
    let TyKind::Adt(adt, args) = ty.kind() else {
        return None;
    };
    let owned_box = OWNED_BOX.get_or_init(|| {
        flux_hir::Core::get()
            .items
            .lang_enums
            .get(&Symbol::intern("owned_box"))
            .copied()
    });
    (Some(adt.strukt) == *owned_box)
        .then(|| args.first().copied())
        .flatten()
}

/// The owner of a supported pinned method receiver.
pub fn pin_pointee(items: &ItemTree, ty: Ty) -> Option<Ty> {
    let TyKind::Adt(adt, args) = ty.kind() else {
        return None;
    };
    if !is_lang_struct(items, adt.strukt, "pin") {
        return None;
    }
    let pointer = *args.first()?;
    match pointer.kind() {
        TyKind::Ref(_, target) => Some(target),
        _ => box_pointee(pointer),
    }
}

/// Whether `strukt` is `core`'s struct with `#[lang = "{lang}"]`.
pub fn is_lang_struct(items: &ItemTree, strukt: StructId, lang: &str) -> bool {
    items.lang_enums.get(&Symbol::intern(lang)) == Some(&strukt)
}
