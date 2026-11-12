//! Recover associated output occurrences from existing signature region tables.
use super::*;

/// An implied output-outlives requirement with its original selection inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionOutlives {
    pub ty: Ty,
    pub regions: Box<[Region]>,
    pub target: Region,
}

/// Recover reference-implied projection requirements without changing storage.
/// A projection's inputs identify the occurrence; they are not stored output
/// lifetimes and must not themselves be required to outlive the reference.
pub fn type_projection_outlives(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    row: &[Region],
) -> Vec<ProjectionOutlives> {
    fn walk(
        items: &ItemTree,
        sigs: &Signatures,
        active: &mut Vec<flux_hir::StructId>,
        ty: Ty,
        row: &[Region],
        bounds: &[Region],
        out: &mut Vec<ProjectionOutlives>,
    ) -> Option<()> {
        if row.len() != slots(items, ty) {
            return None;
        }
        if let TyKind::Ref(_, inner) = ty.kind() {
            let mut bounds = bounds.to_vec();
            bounds.push(*row.first()?);
            return walk(items, sigs, active, inner, row.get(1..)?, &bounds, out);
        }
        if let TyKind::Alias(alias) = ty.kind() {
            if has_projection_inputs(items, alias) {
                for &target in bounds {
                    let requirement = ProjectionOutlives {
                        ty,
                        regions: row.into(),
                        target,
                    };
                    if !out.contains(&requirement) {
                        out.push(requirement);
                    }
                }
                let mut at = alias.owner_lifetimes as usize;
                for &arg in alias.args {
                    let end = at.checked_add(slots(items, arg))?;
                    walk(items, sigs, active, arg, row.get(at..end)?, &[], out)?;
                    at = end;
                }
                at += alias.family_lifetimes as usize;
                for &arg in alias.family_args {
                    let end = at.checked_add(slots(items, arg))?;
                    walk(items, sigs, active, arg, row.get(at..end)?, &[], out)?;
                    at = end;
                }
                if at != row.len() {
                    return None;
                }
            }
            return Some(());
        }
        let (children, mut at): (Vec<Ty>, usize) = match ty.kind() {
            TyKind::Adt(adt, args) => (
                args.to_vec(),
                items.structs[adt.strukt].generics.lifetimes.len(),
            ),
            TyKind::Dynamic(object) => (
                object_region_types(items, object).collect(),
                object_own_slots(items, object),
            ),
            TyKind::Closure(_, args) => (vec![ClosureArgs::split(args).upvars], 0),
            TyKind::FnPtr(_) | TyKind::FnDef(_) => return Some(()),
            TyKind::Array(inner, _) => (vec![inner], 0),
            _ => (ty.children().collect(), 0),
        };
        for child in children {
            let end = at.checked_add(slots(items, child))?;
            walk(items, sigs, active, child, row.get(at..end)?, bounds, out)?;
            at = end;
        }
        // Nominal fields can store a projection absent from the type arguments.
        // Reconstruct each field's original row before substituting its types.
        if let TyKind::Adt(adt, args) = ty.kind()
            && !active.contains(&adt.strukt)
            && outlives_structs_with_projections(sigs, ty).contains(&adt.strukt)
        {
            let metadata = sigs.regions.structs.get(adt.strukt)?;
            let own = metadata.lifetimes as usize;
            let mut at = own;
            let mut arguments = Vec::new();
            for &arg in args {
                let end = at.checked_add(slots(items, arg))?;
                arguments.push(row.get(at..end)?.to_vec());
                at = end;
            }
            active.push(adt.strukt);
            for (&field, table) in sigs
                .structs
                .get(adt.strukt)?
                .iter()
                .zip(metadata.fields.iter())
            {
                let mut field_row = Vec::new();
                instantiate(
                    items,
                    field,
                    table,
                    &mut |region| match region {
                        Region::Param(index) => {
                            row.get(index as usize).copied().unwrap_or(Region::Erased)
                        }
                        region => region,
                    },
                    &|index| arguments.get(index as usize).cloned().unwrap_or_default(),
                    &mut field_row,
                );
                walk(
                    items,
                    sigs,
                    active,
                    field.subst(args),
                    &field_row,
                    bounds,
                    out,
                )?;
            }
            active.pop();
        }
        Some(())
    }
    let mut out = Vec::new();
    if walk(items, sigs, &mut Vec::new(), ty, row, &[], &mut out).is_none() {
        out.clear();
    }
    out
}

/// Recover reference-implied projections in every parameter and return type.
pub fn signature_projection_outlives(
    items: &ItemTree,
    sigs: &Signatures,
    function: flux_hir::FunctionId,
) -> Vec<ProjectionOutlives> {
    let Some(regions) = sigs.regions.fns.get(function) else {
        return Vec::new();
    };
    let signature = &sigs.fns[function];
    signature
        .params
        .iter()
        .chain([&signature.ret])
        .zip(regions.params.iter().chain([&regions.ret]))
        .flat_map(|(&ty, row)| type_projection_outlives(items, sigs, ty, row))
        .collect()
}
