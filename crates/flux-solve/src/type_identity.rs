//! Opaque runtime type identities. Keys are types, never diagnostic spellings.

use flux_hir::ItemTree;
use rustc_hash::FxHashMap;

use crate::{FnPtrRegions, FnRegion, Signatures, Ty, TyKind};

/// Identity after substitution and normalization. Hash collisions are resolved
/// by full structural key equality before an ID is assigned.
pub fn of(items: &ItemTree, sigs: &Signatures, ty: Ty) -> Result<u128, &'static str> {
    let key = canonical(items, sigs, ty, 0)?;
    let mut ids = sigs.type_ids.0.lock().unwrap();
    let next = ids.len() as u128 + 1;
    Ok(*ids.entry(key).or_insert(next))
}

fn canonical(items: &ItemTree, sigs: &Signatures, ty: Ty, depth: u32) -> Result<Ty, &'static str> {
    if depth > 128 {
        return Err("type identity recursion limit reached");
    }
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
                types.push(canonical(items, sigs, ty, depth + 1)?);
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
            return Ok(Ty::new(TyKind::FnPtr(sig)));
        }
        _ => {}
    }
    let mut failure = None;
    let ty = ty.map_children(|child| match canonical(items, sigs, child, depth + 1) {
        Ok(child) => child,
        Err(error) => {
            failure = Some(error);
            Ty::ERROR
        }
    });
    failure.map_or(Ok(ty), Err)
}
