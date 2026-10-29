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
    let mut rows = Vec::new();
    for (&ty, recipe) in sig.inputs_output.iter().zip(r.inputs_output) {
        let mut row = Vec::new();
        for term in *recipe {
            match *term {
                FnRegion::Static => row.push(static_region),
                FnRegion::Bound(i) => row.push(*bound.get(i as usize)?),
                FnRegion::Free(i) => row.push(*free.get(i as usize)?),
                FnRegion::Type(i) => row.extend_from_slice(captures.get(i as usize)?),
            }
        }
        if row.len() != crate::regions::slots(items, ty) {
            return None;
        }
        rows.push(row);
    }
    Some(rows)
}
