//! Opaque runtime type identities. Keys are types, never diagnostic spellings.

use flux_hir::ItemTree;
use rustc_hash::FxHashMap;

use crate::{FnPtrRegions, FnRegion, Signatures, Ty, TyKind};

/// Identity after substitution and normalization, with stable fingerprints
/// and full structural key checks for fingerprint collisions.
pub fn of(items: &ItemTree, sigs: &Signatures, ty: Ty) -> Result<u128, &'static str> {
    let key = canonical(items, sigs, ty, 0, &mut FxHashMap::default())?;
    sigs.type_ids.of(items, key)
}

fn canonical(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    depth: u32,
    memo: &mut FxHashMap<(Ty, u32), Ty>,
) -> Result<Ty, &'static str> {
    if depth > 128 {
        return Err("type identity recursion limit reached");
    }
    if let Some(&result) = memo.get(&(ty, depth)) {
        return Ok(result);
    }
    let input = ty;
    let ty = crate::traits::reveal(sigs, crate::traits::normalize(items, sigs, &[], ty));
    match ty.kind() {
        TyKind::Error
        | TyKind::Infer(_)
        | TyKind::Param(_)
        | TyKind::Alias(_)
        | TyKind::Opaque(_) => {
            return Err("type identity requires a fully resolved type");
        }
        TyKind::FnPtr(mut sig) => {
            let mut types = Vec::new();
            for &ty in sig.inputs_output {
                types.push(canonical(items, sigs, ty, depth + 1, memo)?);
            }
            let mut rows = Vec::new();
            if let Some(recipe) = sig.regions {
                let own = vec![
                    FnRegion::Static;
                    recipe.free as usize
                        + recipe
                            .types
                            .iter()
                            .map(|&ty| crate::regions::slots(items, ty))
                            .sum::<usize>()
                ];
                let bound: Vec<_> = (0..recipe.bound).map(FnRegion::Bound).collect();
                let expanded =
                    flux_ty_ir::fn_regions::instantiate(items, sig, &own, &bound, FnRegion::Static)
                        .ok_or("function pointer identity has invalid region metadata")?;
                // Only observed bound slots matter. Declaration names and
                // unused binders do not contribute to Rust type identity.
                let mut names = FxHashMap::default();
                for row in expanded {
                    let row: Vec<_> = row
                        .into_iter()
                        .map(|region| match region {
                            FnRegion::Bound(index) => {
                                let next = names.len() as u32;
                                FnRegion::Bound(*names.entry(index).or_insert(next))
                            }
                            _ => FnRegion::Static,
                        })
                        .collect();
                    rows.push(row);
                }
                sig.regions = if rows.iter().all(|r| r.is_empty()) {
                    None
                } else {
                    Some(FnPtrRegions::intern(names.len() as u32, 0, &rows, &[]))
                };
            } else if sig
                .inputs_output
                .iter()
                .any(|&ty| crate::regions::slots(items, ty) != 0)
            {
                return Err("function pointer identity requires region metadata");
            }
            sig.inputs_output = Ty::list(&types);
            let result = Ty::new(TyKind::FnPtr(sig));
            memo.insert((input, depth), result);
            return Ok(result);
        }
        _ => {}
    }
    let mut failure = None;
    let ty = ty.map_children(
        |child| match canonical(items, sigs, child, depth + 1, memo) {
            Ok(child) => child,
            Err(error) => {
                failure = Some(error);
                Ty::ERROR
            }
        },
    );
    if let Some(error) = failure {
        return Err(error);
    }
    memo.insert((input, depth), ty);
    Ok(ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_tuple_dag_canonicalization_visits_distinct_type_depth_pairs() {
        let mut ty = Ty::BOOL;
        for _ in 0..40 {
            ty = Ty::tuple(&[ty, ty]);
        }
        let items = ItemTree::default();
        let sigs = Signatures::default();
        let mut memo = FxHashMap::default();
        assert_eq!(canonical(&items, &sigs, ty, 0, &mut memo).unwrap(), ty);
        assert_eq!(
            memo.len(),
            41,
            "canonicalization should cache each DAG node"
        );
        assert_eq!(
            of(&items, &sigs, ty).unwrap(),
            flux_ty_ir::type_identity::stable_id(&items, &[], ty).unwrap()
        );
    }
    #[test]
    fn memoized_shallow_subtree_does_not_skip_deeper_recursion_limit() {
        let shared = Ty::raw_ptr(crate::Mutability::Not, Ty::BOOL);
        let mut deep = shared;
        for _ in 0..127 {
            deep = Ty::raw_ptr(crate::Mutability::Not, deep);
        }
        let root = Ty::tuple(&[shared, deep]);
        let mut memo = FxHashMap::default();
        assert_eq!(
            canonical(
                &ItemTree::default(),
                &Signatures::default(),
                root,
                0,
                &mut memo
            ),
            Err("type identity recursion limit reached")
        );
        assert!(
            memo.contains_key(&(shared, 1)),
            "shallow occurrence must succeed before deep occurrence"
        );
        assert!(
            !memo.contains_key(&(shared, 128)),
            "failed deep occurrence must not be cached"
        );
    }
}
