//! Early-bound lifetime identity when comparing trait and impl methods.
use super::*;

/// Method lifetimes mentioned in predicates, or only in output types, belong
/// to the early binder. Entirely unused late lifetimes do not add parameters.
pub fn method_early_lifetimes(
    items: &ItemTree,
    sigs: &Signatures,
    function: flux_hir::FunctionId,
) -> Option<Vec<u32>> {
    let data = &items.functions[function];
    let regions = sigs.regions.fns.get(function)?;
    let signature = sigs.fns.get(function)?;
    let owner = regions.owner as usize;
    let count = data.generics.lifetimes.len();
    if regions
        .names
        .get(owner..owner + count)?
        .iter()
        .map(|name| name.name)
        .ne(data.generics.lifetimes.iter().map(|name| Some(name.name)))
        || signature.params.len() != regions.params.len()
    {
        return None;
    }
    if count == 0 {
        return Some(Vec::new());
    }
    let mut inputs = Vec::new();
    for (&ty, row) in signature.params.iter().zip(regions.params.iter()) {
        constrained_inputs(items, ty, row, &mut inputs)?;
    }
    let mut output = Vec::new();
    if !resolve::callback_type_lifetimes(&data.ret, &mut output) {
        return None;
    }
    let mut written = Vec::new();
    for predicate in &data.generics.predicates {
        let start = written.len();
        if !resolve::callback_type_lifetimes(&predicate.ty, &mut written)
            || !resolve::callback_path_lifetimes(&predicate.bound.path, &mut written)
        {
            return None;
        }
        let mut local = written.split_off(start);
        local.retain(|lifetime| !predicate.bound.bound_lifetimes.contains(&lifetime.name));
        written.extend(local);
    }
    for predicate in &data.generics.outlives {
        written.extend([predicate.longer, predicate.shorter]);
    }
    for predicate in &data.generics.type_outlives {
        written.push(predicate.lifetime);
        if !resolve::callback_type_lifetimes(&predicate.ty, &mut written) {
            return None;
        }
    }
    Some(
        data.generics
            .lifetimes
            .iter()
            .enumerate()
            .filter_map(|(index, lifetime)| {
                let index = (owner + index) as u32;
                ((!inputs.contains(&Region::Param(index))
                    && output.iter().any(|written| written.name == lifetime.name))
                    || written.iter().any(|written| written.name == lifetime.name))
                .then_some(index)
            })
            .collect(),
    )
}

fn constrained_inputs(
    items: &ItemTree,
    ty: Ty,
    row: &[Region],
    out: &mut Vec<Region>,
) -> Option<()> {
    if row.len() != slots(items, ty) {
        return None;
    }
    if matches!(ty.kind(), TyKind::Alias(_)) {
        // A projection's selection arguments do not constrain its output.
        return Some(());
    }
    if let TyKind::FnPtr(signature) = ty.kind() {
        if signature.regions.is_none()
            && signature
                .inputs_output
                .iter()
                .any(|&ty| slots(items, ty) != 0)
        {
            return None;
        }
        out.extend_from_slice(row);
        return Some(());
    }
    if matches!(ty.kind(), TyKind::Dynamic(_)) {
        out.extend_from_slice(row);
        return Some(());
    }
    let direct = match ty.kind() {
        TyKind::Ref(..) => 1,
        TyKind::Adt(adt, _) => items.structs[adt.strukt].generics.lifetimes.len(),
        _ => 0,
    };
    out.extend_from_slice(row.get(..direct)?);
    let mut at = direct;
    for child in ty.children() {
        let count = slots(items, child);
        constrained_inputs(items, child, row.get(at..at + count)?, out)?;
        at += count;
    }
    (at == row.len()).then_some(())
}
