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

pub(crate) fn tail(fields: Option<&LayeredArenaMap<Struct, &'static [Ty]>>, mut ty: Ty, is_enum: impl Fn(flux_hir::StructId) -> bool) -> Ty {
    let mut seen = Vec::new();
    while let TyKind::Adt(adt, args) = ty.kind() {
        if is_enum(adt.strukt) { break; }
        if seen.len() >= 256 || seen.contains(&ty) { return Ty::ERROR; }
        seen.push(ty);
        let Some(last) = fields.and_then(|fields| fields.get(adt.strukt)).and_then(|fields| fields.last()) else { break; };
        ty = last.subst(args);
    }
    ty
}

impl Signatures {
    /// The terminal unsized field. Sized types may also have a terminal field;
    /// callers distinguish it by metadata or by their generic Sized bounds.
    pub fn structural_tail(&self, ty: Ty) -> Ty {
        if matches!(ty.kind(), TyKind::Adt(adt, _) if self.variant_starts.get(adt.strukt).is_some()) { return ty; }
        tail(Some(&self.structs), ty, |id| self.variant_starts.get(id).is_some())
    }

    pub fn metadata(&self, ty: Ty) -> Metadata {
        match self.structural_tail(ty).kind() {
            TyKind::Slice(_) | TyKind::Str => Metadata::Length,
            TyKind::Dynamic(_) => Metadata::VTable,
            TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) => Metadata::Generic(self.structural_tail(ty)),
            _ => Metadata::Thin,
        }
    }

    /// The array and slice element types at matching structural tails.
    /// Prefix fields must be unchanged by this built-in unsizing coercion.
    pub fn array_unsize(&self, from: Ty, to: Ty) -> Option<(Ty, Ty)> {
        let mut pair = (from, to);
        for _ in 0..256 {
            match (pair.0.kind(), pair.1.kind()) {
                (TyKind::Array(a, _), TyKind::Slice(b)) => return Some((a, b)),
                (TyKind::Adt(a, _), TyKind::Adt(b, _)) if a == b => {
                    let (from, to) = (self.fields(pair.0)?, self.fields(pair.1)?);
                    let last = from.len().checked_sub(1)?;
                    if from.len() != to.len() || from[..last] != to[..last] { return None; }
                    pair = (from[last], to[last]);
                }
                _ => return None,
            }
        }
        None
    }

    /// Definitely unsized after substitution. Generic tails additionally need
    /// the declaration's Sized bounds, handled by `maybe_unsized`.
    pub fn is_unsized(&self, ty: Ty) -> bool {
        matches!(self.metadata(ty), Metadata::Length | Metadata::VTable)
    }
}
