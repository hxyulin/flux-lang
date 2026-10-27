//! Packed fields can be moved unaligned, but a reference promises alignment.
use flux_hir::{Diagnostic, ItemTree};
use flux_ty::{Signatures, Ty, TyKind};

use crate::{Body, Place, PlaceTy, ProjectionElem, Rvalue, StatementKind};

/// The strongest packing cap on a place since its last pointer dereference.
pub(crate) fn packing(
    items: &ItemTree,
    sigs: &Signatures,
    body: &Body,
    place: &Place,
) -> Option<u32> {
    let mut ty = PlaceTy::from(body.locals[place.local].ty);
    let mut cap: Option<u32> = None;
    for &projection in place.projection.iter() {
        match projection {
            ProjectionElem::Deref | ProjectionElem::BoxDeref => cap = None,
            ProjectionElem::Field(_) => {
                if let TyKind::Adt(adt, _) = ty.ty.kind()
                    && let Some(pack) = items.structs[adt.strukt].repr_pack
                {
                    cap = Some(cap.map_or(pack, |outer| outer.min(pack)));
                }
            }
            _ => {}
        }
        ty = ty.project(projection, sigs);
    }
    cap
}

/// Unknown generic alignment must not be assumed safe beneath a packing cap.
fn alignment(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    active: &mut Vec<Ty>,
    parameters: &[bool],
    bounds: &[flux_ty::Predicate],
) -> Option<u64> {
    if active.contains(&ty) {
        return None;
    }
    match ty.kind() {
        TyKind::Ref(_, pointee) | TyKind::RawPtr(_, pointee) => {
            if matches!(sigs.metadata(pointee), flux_ty::Metadata::Generic(_))
                && flux_ty::maybe_unsized(items, sigs, pointee, parameters, bounds)
            {
                None
            } else {
                Some(8)
            }
        }
        TyKind::FnPtr(_) | TyKind::FnDef(_) => Some(8),
        TyKind::Array(elem, _) | TyKind::Slice(elem) => {
            alignment(items, sigs, elem, active, parameters, bounds)
        }
        TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_) => None,
        _ if !ty.has_params() => Some(
            crate::layout::Layouts::new(items, sigs, &Default::default())
                .of(ty)
                .align,
        ),
        TyKind::Adt(adt, _) if items.structs[adt.strukt].repr_pack.is_some() => {
            Some(u64::from(items.structs[adt.strukt].repr_pack.unwrap()))
        }
        _ => {
            active.push(ty);
            let fields = sigs.fields(ty)?;
            let result = fields.iter().try_fold(1, |align, &field| {
                alignment(items, sigs, field, active, parameters, bounds)
                    .map(|field| align.max(field))
            });
            active.pop();
            result
        }
    }
}

pub(crate) fn check(body: &Body, items: &ItemTree, sigs: &Signatures) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    // Most bodies never borrow a packed field. Their declaration environment
    // is needed only to determine the alignment of a generic packed target.
    let mut environment = None;
    for block in body.blocks.values() {
        for statement in &block.statements {
            if let StatementKind::Assign(
                _,
                Rvalue::Ref {
                    raw: false, place, ..
                }
                | Rvalue::Len {
                    place,
                    borrows: true,
                },
            ) = &statement.kind
                && let Some(cap) = packing(items, sigs, body, place)
            {
                let (parameters, bounds) = environment.get_or_insert_with(|| {
                    let parameters = body
                        .owner
                        .map(|owner| {
                            let data = &items.functions[owner];
                            flux_ty::unsized_params(items, data.owner, &data.generics)
                        })
                        .unwrap_or_default();
                    let bounds = body
                        .owner
                        .map(|owner| flux_ty::traits::predicates_of(items, sigs, owner))
                        .unwrap_or_default();
                    (parameters, bounds)
                });
                let required = alignment(
                    items,
                    sigs,
                    body.place_ty(place, sigs),
                    &mut Vec::new(),
                    parameters,
                    bounds,
                );
                if required.is_none_or(|align| align > u64::from(cap)) {
                    errors.push(Diagnostic::new(
                        statement.range,
                        "reference to packed field is unaligned",
                    ));
                }
            }
        }
    }
    errors
}

/// Rust forbids an explicitly aligned type anywhere in a packed by-value field.
pub(crate) fn check_declarations(items: &ItemTree, sigs: &Signatures) -> Vec<Diagnostic> {
    fn aligned(
        items: &ItemTree,
        sigs: &Signatures,
        ty: Ty,
        active: &mut Vec<flux_hir::StructId>,
    ) -> bool {
        match ty.kind() {
            TyKind::Array(elem, _) | TyKind::Slice(elem) => aligned(items, sigs, elem, active),
            TyKind::Tuple(fields) => fields
                .iter()
                .any(|&field| aligned(items, sigs, field, active)),
            TyKind::Adt(adt, _) => {
                if items.structs[adt.strukt].repr_align > 1 {
                    return true;
                }
                if active.contains(&adt.strukt) {
                    return false;
                }
                active.push(adt.strukt);
                let found = sigs.fields(ty).is_some_and(|fields| {
                    fields
                        .iter()
                        .any(|&field| aligned(items, sigs, field, active))
                }) || sigs.variants(ty).is_some_and(|variants| {
                    variants
                        .iter()
                        .flat_map(|fields| fields.iter())
                        .any(|&field| aligned(items, sigs, field, active))
                });
                active.pop();
                found
            }
            _ => false,
        }
    }
    fn tail_needs_drop(
        items: &ItemTree,
        sigs: &Signatures,
        bounds: &[flux_ty::Predicate],
        ty: Ty,
        active: &mut Vec<Ty>,
    ) -> bool {
        if active.contains(&ty) {
            return false;
        }
        match ty.kind() {
            TyKind::Slice(elem) => flux_ty::traits::needs_drop(items, sigs, bounds, elem),
            TyKind::Adt(adt, _) if !flux_ty::is_lang_struct(items, adt.strukt, "manually_drop") => {
                if flux_ty::traits::drop_impl(items, sigs, adt.strukt).is_some() {
                    return true;
                }
                active.push(ty);
                let result = sigs.fields(ty).is_some_and(|fields| {
                    fields
                        .iter()
                        .any(|&field| tail_needs_drop(items, sigs, bounds, field, active))
                });
                active.pop();
                result
            }
            _ => flux_ty::traits::needs_drop(items, sigs, bounds, ty),
        }
    }
    let mut errors = Vec::new();
    for (id, data) in items.structs.iter() {
        if data.repr_pack.is_some() {
            let parameters = flux_ty::unsized_params(items, None, &data.generics);
            let bounds = sigs.struct_predicates.get(id).map_or(&[][..], |b| b);
            if let Some(&tail) = sigs.structs.get(id).and_then(|fields| fields.last())
                && flux_ty::maybe_unsized(items, sigs, tail, &parameters, bounds)
                && tail_needs_drop(items, sigs, bounds, tail, &mut Vec::new())
            {
                let mut error =
                    Diagnostic::new(data.def_range, "unsized packed field may require drop");
                error.file = data.loc.file;
                errors.push(error);
            }
        }
        if data.repr_pack.is_some()
            && sigs.structs.get(id).is_some_and(|fields| {
                fields
                    .iter()
                    .any(|&field| aligned(items, sigs, field, &mut vec![id]))
            })
        {
            let mut error = Diagnostic::new(
                data.def_range,
                "packed type cannot transitively contain an aligned type",
            );
            error.file = data.loc.file;
            errors.push(error);
        }
    }
    errors
}
