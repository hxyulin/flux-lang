//! Instances of the canonical region recipes of bare function-pointer
//! types. `flux-ty` builds the recipes.
use crate::FnRegion;
use flux_hir::ItemTree;

/// Instantiate only a pointer's relative recipe. Bound variables are supplied
/// freshly by the call/proof; own variables are free slots then captured types.
pub fn instantiate<R: Copy>(
    items: &ItemTree,
    sig: crate::FnSigTy,
    own: &[R],
    bound: &[R],
    static_region: R,
) -> Option<Vec<Vec<R>>> {
    instantiate_with(items, sig, own, bound, static_region, |_, _| None)
}

/// Instantiate associated outputs through a caller-owned selection resolver.
/// Projections form a DAG: each input recipe can use only earlier entries.
pub fn instantiate_with<R: Copy>(
    items: &ItemTree,
    sig: crate::FnSigTy,
    own: &[R],
    bound: &[R],
    static_region: R,
    mut project: impl FnMut(crate::AliasTy, &[R]) -> Option<Vec<R>>,
) -> Option<Vec<Vec<R>>> {
    let r = sig.regions?;
    if bound.len() != r.bound as usize || r.inputs_output.len() != sig.inputs_output.len() {
        return None;
    }
    let free = own.get(..r.free as usize)?;
    let mut at = r.free as usize;
    let mut captures = Vec::new();
    for &ty in r.types {
        let n = crate::regions::slots(items, ty);
        captures.push(own.get(at..at + n)?);
        at += n;
    }
    if at != own.len() {
        return None;
    }
    fn expand<R: Copy>(
        recipe: &[FnRegion],
        free: &[R],
        bound: &[R],
        captures: &[&[R]],
        projections: &[Vec<R>],
        static_region: R,
    ) -> Option<Vec<R>> {
        let mut row = Vec::new();
        for &term in recipe {
            match term {
                FnRegion::Static => row.push(static_region),
                FnRegion::Bound(i) => row.push(*bound.get(i as usize)?),
                FnRegion::Free(i) => row.push(*free.get(i as usize)?),
                FnRegion::Type(i) => row.extend_from_slice(captures.get(i as usize)?),
                FnRegion::Projection(i) => row.extend_from_slice(projections.get(i as usize)?),
            }
        }
        Some(row)
    }
    let mut projections = Vec::new();
    for projection in r.projections {
        let inputs = expand(
            projection.inputs,
            free,
            bound,
            &captures,
            &projections,
            static_region,
        )?;
        projections.push(project(projection.alias, &inputs)?);
    }
    let mut rows = Vec::new();
    for (&ty, recipe) in sig.inputs_output.iter().zip(r.inputs_output) {
        let row = expand(recipe, free, bound, &captures, &projections, static_region)?;
        if row.len() != crate::regions::slots(items, ty) {
            return None;
        }
        rows.push(row);
    }
    Some(rows)
}
