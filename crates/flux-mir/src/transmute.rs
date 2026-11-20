//! Definition-time size checks for transmute, before generic instantiation.
use flux_hir::{Diagnostic, ItemTree};
use flux_ty::{Signatures, Ty, TyKind};

use crate::{Body, Constant, Operand, Rvalue, StatementKind, TerminatorKind};

pub(crate) fn check(
    items: &ItemTree,
    sigs: &Signatures,
    body: &Body,
    unsized_parameters: &[bool],
    discriminants: &crate::Discriminants,
) -> Vec<Diagnostic> {
    let layouts = crate::layout::Layouts::new(items, sigs, discriminants);
    let mut errors = Vec::new();
    let sites = body.blocks.values().flat_map(|block| {
        let items = block.statements.iter().filter_map(|stmt| {
            let StatementKind::Assign(_, Rvalue::Use(Operand::Const(Constant::FnItem(ty)))) =
                &stmt.kind
            else {
                return None;
            };
            let TyKind::FnDef(item) = ty.kind() else {
                return None;
            };
            Some((item.function, item.args, stmt.range))
        });
        let call = match &block.terminator.kind {
            TerminatorKind::Call {
                func, generic_args, ..
            } => Some((*func, *generic_args, block.terminator.range)),
            _ => None,
        };
        items.chain(call)
    });
    for (func, generic_args, range) in sites {
        let data = &items.functions[func];
        if data.intrinsic != Some(flux_hir::Intrinsic::Transmute) {
            continue;
        }
        let &[from, to] = generic_args else {
            continue;
        };
        if from == to {
            continue;
        }
        // The same possibly-unsized pointee has the same metadata width,
        // independent of pointer mutability and reference validity.
        if matches!((from.kind(), to.kind()),
            (TyKind::Ref(_, a) | TyKind::RawPtr(_, a), TyKind::Ref(_, b) | TyKind::RawPtr(_, b)) if a == b)
        {
            continue;
        }
        let known = |ty| known_layout(items, sigs, ty, unsized_parameters, 0);
        let dependent = !known(from) || !known(to);
        // Constant expressions in enum discriminants are evaluated after
        // initial MIR checking. Compare these layouts once their values exist.
        let unresolved = !dependent
            && (unresolved_discriminants(items, sigs, discriminants, from, 0)
                || unresolved_discriminants(items, sigs, discriminants, to, 0));
        if dependent || (!unresolved && layouts.of(from).size != layouts.of(to).size) {
            errors.push(Diagnostic::new(
                range,
                "cannot transmute between types of different sizes, or dependently-sized types",
            ));
        }
    }
    errors
}

fn known_layout(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    unsized_parameters: &[bool],
    depth: u32,
) -> bool {
    if depth > 128 {
        return false;
    }
    let known = |ty| known_layout(items, sigs, ty, unsized_parameters, depth + 1);
    match ty.kind() {
        TyKind::Param(_)
        | TyKind::Alias(_)
        | TyKind::Opaque(_)
        | TyKind::Infer(_)
        | TyKind::Error => false,
        TyKind::Ref(_, pointee) | TyKind::RawPtr(_, pointee) => {
            match sigs.structural_tail(pointee).kind() {
                TyKind::Param(p) => unsized_parameters.get(p.index as usize) == Some(&false),
                // The declaration is enough to know the metadata width of
                // pointers to a Sized associated type, including a struct
                // whose last field has that type.
                TyKind::Alias(alias) => items.traits[alias.trait_]
                    .types
                    .iter()
                    .find(|associated| associated.name == alias.name)
                    .is_some_and(|associated| !associated.maybe_unsized),
                TyKind::Opaque(_) | TyKind::Infer(_) | TyKind::Error => false,
                _ => true,
            }
        }
        TyKind::FnPtr(_) | TyKind::FnDef(_) => true,
        TyKind::Array(elem, len) => len.as_usize().is_some() && known(elem),
        TyKind::Adt(..) => {
            if let Some(variants) = flux_solve::variants(sigs, ty) {
                variants
                    .iter()
                    .flat_map(|fields| fields.iter())
                    .copied()
                    .all(known)
            } else {
                flux_solve::fields(sigs, ty).is_some_and(|fields| fields.iter().copied().all(known))
            }
        }
        TyKind::Closure(_, args) => flux_ty::ClosureArgs::split(args)
            .upvar_types()
            .iter()
            .copied()
            .all(known),
        TyKind::Tuple(fields) => fields.iter().copied().all(known),
        _ => !sigs.is_unsized(ty),
    }
}

fn unresolved_discriminants(
    items: &ItemTree,
    sigs: &Signatures,
    discriminants: &crate::Discriminants,
    ty: Ty,
    depth: u32,
) -> bool {
    if depth > 128 {
        return true;
    }
    let unresolved = |ty| unresolved_discriminants(items, sigs, discriminants, ty, depth + 1);
    match ty.kind() {
        TyKind::Ref(..) | TyKind::RawPtr(..) | TyKind::FnPtr(_) | TyKind::FnDef(_) => false,
        TyKind::Adt(adt, _) => {
            if let Some(variants) = flux_solve::variants(sigs, ty) {
                (0..variants.len()).any(|i| {
                    crate::discriminant_bits(items, discriminants, adt.strukt, i as u32).is_none()
                }) || variants
                    .iter()
                    .any(|fields| fields.iter().copied().any(unresolved))
            } else {
                flux_solve::fields(sigs, ty)
                    .is_some_and(|fields| fields.iter().copied().any(unresolved))
            }
        }
        TyKind::Array(elem, _) => unresolved(elem),
        TyKind::Tuple(fields) => fields.iter().copied().any(unresolved),
        TyKind::Closure(_, args) => flux_ty::ClosureArgs::split(args)
            .upvar_types()
            .iter()
            .copied()
            .any(unresolved),
        _ => false,
    }
}
