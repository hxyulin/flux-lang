//! Pointer metadata follows a struct's substituted final field.
use flux_arena::LayeredArenaMap;
use flux_hir::Struct;

use crate::{Signatures, Ty, TyKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metadata {
    Thin,
    Length,
    VTable,
    /// A generic tail whose metadata is determined by substitution.
    Generic(Ty),
}

pub fn tail(
    fields: Option<&LayeredArenaMap<Struct, &'static [Ty]>>,
    mut ty: Ty,
    is_enum: impl Fn(flux_hir::StructId) -> bool,
) -> Ty {
    let mut seen = Vec::new();
    loop {
        // A tuple is unsized as its last element is.
        if let TyKind::Tuple([.., last]) = ty.kind() {
            ty = *last;
            continue;
        }
        let TyKind::Adt(adt, args) = ty.kind() else {
            break;
        };
        if is_enum(adt.strukt) {
            break;
        }
        if seen.len() >= 256 || seen.contains(&ty) {
            return Ty::ERROR;
        }
        seen.push(ty);
        let Some(last) = fields
            .and_then(|fields| fields.get(adt.strukt))
            .and_then(|fields| fields.last())
        else {
            break;
        };
        ty = last.subst(args);
    }
    ty
}

impl Signatures {
    /// The terminal unsized field. Sized types may also have a terminal field;
    /// callers distinguish it by metadata or by their generic Sized bounds.
    pub fn structural_tail(&self, ty: Ty) -> Ty {
        if matches!(ty.kind(), TyKind::Adt(adt, _) if self.variant_starts.get(adt.strukt).is_some())
        {
            return ty;
        }
        tail(Some(&self.structs), ty, |id| {
            self.variant_starts.get(id).is_some()
        })
    }

    pub fn metadata(&self, ty: Ty) -> Metadata {
        match self.structural_tail(ty).kind() {
            TyKind::Slice(_) | TyKind::Str | TyKind::CStr => Metadata::Length,
            TyKind::Dynamic(_) => Metadata::VTable,
            TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) => {
                Metadata::Generic(self.structural_tail(ty))
            }
            _ => Metadata::Thin,
        }
    }

    /// Definitely unsized after substitution. Generic tails additionally need
    /// the declaration's Sized bounds, handled by `maybe_unsized`.
    pub fn is_unsized(&self, ty: Ty) -> bool {
        matches!(self.metadata(ty), Metadata::Length | Metadata::VTable)
    }
}

/// The vtable-bearing tail of a single-field transparent wrapper.
/// Restricting wrappers to one type parameter and no lifetime parameters
/// preserves both the data address and the tail's complete occurrence row.
pub fn transparent_object_tail(
    items: &flux_hir::ItemTree,
    fields: &LayeredArenaMap<Struct, &'static [Ty]>,
    mut ty: Ty,
) -> Option<Ty> {
    for _ in 0..256 {
        if ty.is_dyn() {
            return Some(ty);
        }
        ty = transparent_field(items, fields, ty)?;
    }
    None
}

/// Peels the same transparent wrappers from both ends of an object coercion.
pub fn transparent_object_unsize(
    items: &flux_hir::ItemTree,
    fields: &LayeredArenaMap<Struct, &'static [Ty]>,
    mut source: Ty,
    mut target: Ty,
) -> Option<(Ty, Ty)> {
    for _ in 0..256 {
        if target.is_dyn() {
            return Some((source, target));
        }
        match (source.kind(), target.kind()) {
            (TyKind::Adt(a, _), TyKind::Adt(b, _)) if a == b => {}
            _ => return None,
        }
        source = transparent_field(items, fields, source)?;
        target = transparent_field(items, fields, target)?;
    }
    None
}

fn transparent_field(
    items: &flux_hir::ItemTree,
    fields: &LayeredArenaMap<Struct, &'static [Ty]>,
    ty: Ty,
) -> Option<Ty> {
    let TyKind::Adt(adt, args) = ty.kind() else {
        return None;
    };
    let data = &items.structs[adt.strukt];
    if data.is_enum()
        || data.is_union
        || args.len() != 1
        || !data.generics.lifetimes.is_empty()
        || (!data.repr_transparent
            && items
                .lang_enums
                .get(&flux_intern::Symbol::intern("manually_drop"))
                != Some(&adt.strukt))
    {
        return None;
    }
    let [field] = *fields.get(adt.strukt)? else {
        return None;
    };
    Some(field.subst(args))
}
