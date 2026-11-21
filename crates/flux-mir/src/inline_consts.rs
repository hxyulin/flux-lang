//! Independent evaluation frames for closed inline constants.
use crate::{Body, Operand, Rvalue, StatementKind, TerminatorKind};
use flux_hir::ItemTree;
use flux_ty::{ClosureOwner, Ty, TyKind};

pub(crate) fn parameters(items: &ItemTree, owner: ClosureOwner) -> usize {
    match owner {
        ClosureOwner::Function(f) => {
            let function = &items.functions[f];
            flux_solve::traits::owner_param_count(items, function.owner) + function.generics.len()
        }
        ClosureOwner::Const(c) => {
            flux_solve::traits::owner_param_count(items, items.consts[c].owner)
        }
    }
}

/// An independent evaluation frame may use its owner's parameters in item
/// references. Its storage types and region constraints are otherwise closed.
pub(crate) fn closed(body: &Body, parameters: usize) -> bool {
    let open = |ty: Ty| ty.has_params() || ty.has_aliases() || ty.has_opaques() || ty.has_infer();
    fn valid_argument(ty: Ty, parameters: usize) -> bool {
        if ty.has_aliases() || ty.has_opaques() || ty.has_infer() {
            return false;
        }
        match ty.kind() {
            TyKind::Param(param) => (param.index as usize) < parameters,
            _ => !ty.has_params() || ty.children().all(|child| valid_argument(child, parameters)),
        }
    }
    let valid_item =
        |ty: Ty| matches!(ty.kind(), TyKind::FnDef(_)) && valid_argument(ty, parameters);
    if body
        .locals
        .values()
        .any(|local| open(local.ty) && !valid_item(local.ty))
    {
        return false;
    }
    let captured = |row: &[flux_ty::regions::Region]| {
        row.iter().any(|region| {
            matches!(
                region,
                flux_ty::regions::Region::Param(_) | flux_ty::regions::Region::Late(_)
            )
        })
    };
    if body
        .region_annotations
        .iter()
        .any(|(_, row, _)| captured(row))
        || body
            .call_regions
            .iter()
            .any(|(_, rows)| rows.iter().any(|(_, row)| captured(row)))
        || body
            .reification_regions
            .iter()
            .any(|(_, rows)| rows.iter().any(|(_, row)| captured(row)))
    {
        return false;
    }
    let operand_open = |operand: &Operand| match operand {
        Operand::Const(crate::Constant::TyConst { value, .. }) => open(*value),
        Operand::Const(
            crate::Constant::Item { args, .. }
            | crate::Constant::ItemRef { args, .. }
            | crate::Constant::Fn { args, .. },
        ) => args.iter().any(|&arg| !valid_argument(arg, parameters)),
        Operand::Const(crate::Constant::FnItem(ty)) => open(*ty) && !valid_item(*ty),
        _ => false,
    };
    for data in body.blocks.values() {
        if let TerminatorKind::Call { generic_args, .. } = &data.terminator.kind
            && generic_args.iter().any(|&arg| open(arg))
        {
            return false;
        }
        for statement in &data.statements {
            if let StatementKind::Assign(_, value) = &statement.kind {
                if matches!(value, Rvalue::Aggregate(crate::AggregateKind::Closure, _)) {
                    return false;
                }
                if matches!(value, Rvalue::OffsetOf(ty, _) if open(*ty)) {
                    return false;
                }
                if value.operands().any(operand_open) {
                    return false;
                }
            }
        }
        if data.terminator.kind.operands().any(operand_open) {
            return false;
        }
    }
    // A nested expression may depend on generics without its parent's output.
    body.inline_constants
        .iter()
        .all(|body| closed(body, parameters))
}

/// The existing promotion path retains value-sensitive qualification for
/// borrowed interior-mutable storage and objects hiding that storage.
pub(crate) fn borrowed_storage_supported(
    items: &flux_hir::ItemTree,
    sigs: &flux_ty::Signatures,
    ty: Ty,
) -> bool {
    fn visit(
        items: &flux_hir::ItemTree,
        sigs: &flux_ty::Signatures,
        ty: Ty,
        seen: &mut rustc_hash::FxHashSet<Ty>,
    ) -> bool {
        if !seen.insert(ty) {
            return true;
        }
        match ty.kind() {
            flux_ty::TyKind::Ref(_, inner) => {
                !inner.is_dyn()
                    && !flux_solve::traits::has_interior_mutability(items, sigs, inner)
                    && visit(items, sigs, inner, seen)
            }
            flux_ty::TyKind::Adt(..) | flux_ty::TyKind::Tuple(_) | flux_ty::TyKind::Array(..) => {
                flux_solve::contents(sigs, ty).is_none_or(|fields| {
                    fields.iter().all(|&field| visit(items, sigs, field, seen))
                })
            }
            _ => true,
        }
    }
    visit(items, sigs, ty, &mut rustc_hash::FxHashSet::default())
}
