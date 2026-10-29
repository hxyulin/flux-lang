//! Storage and query dependencies while an ADT's field type is being resolved.
use flux_hir::{ItemTree, StructId};
use rustc_hash::FxHashSet;

use crate::{Signatures, Ty, TyKind};

/// Whether determining a value's stored layout reaches `owner`'s unfinished
/// fields. Generic arguments alone are not storage: pointers and function
/// values cut this dependency, and marker fields are empty. Arrays still need
/// their element layout at length zero. Substituted associated fields are
/// normalized before walking them.
pub fn stores_adt(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    owner: StructId,
) -> Result<bool, &'static str> {
    let mut pending = vec![(ty, 0u32)];
    let mut seen = FxHashSet::default();
    while let Some((ty, depth)) = pending.pop() {
        let ty = flux_solve::traits::normalize(items, sigs, &[], ty);
        if !seen.insert(ty) {
            continue;
        }
        if depth >= 128 {
            return Err("field constant type dependency exceeds the supported recursion depth");
        }
        match ty.kind() {
            TyKind::Adt(adt, _) if adt.strukt == owner => return Ok(true),
            TyKind::Ref(..) | TyKind::RawPtr(..) | TyKind::FnPtr(_) | TyKind::FnDef(_) => {}
            TyKind::Array(element, _) | TyKind::Slice(element) => {
                pending.push((element, depth + 1))
            }
            _ => pending.extend(
                flux_solve::contents(sigs, ty)
                    .unwrap_or_default()
                    .iter()
                    .map(|&field| (field, depth + 1)),
            ),
        }
    }
    Ok(false)
}

/// Rust's null-pointer constructors query pointer metadata, unlike an ordinary
/// generic function whose signature merely mentions its type parameter.
pub fn queries_pointer_metadata(items: &ItemTree, function: flux_hir::FunctionId) -> bool {
    let function = &items.functions[function];
    items.is_core(function.loc.module)
        && items.modules[function.loc.module].name.as_str() == "ptr"
        && matches!(function.name.as_str(), "null" | "null_mut")
}
