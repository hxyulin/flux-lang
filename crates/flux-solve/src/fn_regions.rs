//! Comparisons of function-pointer types by their region recipes.
use flux_hir::ItemTree;
use flux_ty_ir::fn_regions::instantiate;
use flux_ty_ir::{Ty, TyKind};

/// Equality of binder positions for trait/header matching. Early regions are
/// deliberately all external here; their equality is proven by the caller's
/// region tables. A bound region can never match an external region.
pub fn same_binders(items: &ItemTree, a: Ty, b: Ty) -> bool {
    if a == b || a.has_infer() || b.has_infer() {
        return true;
    }
    if let (TyKind::FnPtr(a), TyKind::FnPtr(b)) = (a.kind(), b.kind()) {
        let shape = |sig: crate::FnSigTy| {
            if let Some(r) = sig.regions {
                let own = vec![None; crate::regions::slots(items, Ty::new(TyKind::FnPtr(sig)))];
                let bound: Vec<_> = (0..r.bound).map(Some).collect();
                instantiate(items, sig, &own, &bound, None)
            } else if sig
                .inputs_output
                .iter()
                .all(|&t| crate::regions::slots(items, t) == 0)
            {
                Some(vec![Vec::new(); sig.inputs_output.len()])
            } else {
                None
            }
        };
        let (Some(x), Some(y)) = (shape(a), shape(b)) else {
            return false;
        };
        if x != y {
            return false;
        }
    }
    // Ordinary type matching/normalization owns all non-callable shapes.
    // This guard only adds binder identity at corresponding callable nodes.
    a.children()
        .zip(b.children())
        .all(|(a, b)| same_binders(items, a, b))
}

/// Type equality after erasing only function-pointer region recipes. This is
/// used before MIR checks callable subtyping, and for MIR storage validation.
pub fn same_shape(a: Ty, b: Ty) -> bool {
    if a == b {
        return true;
    }
    let all =
        |a: &[Ty], b: &[Ty]| a.len() == b.len() && a.iter().zip(b).all(|(&a, &b)| same_shape(a, b));
    match (a.kind(), b.kind()) {
        (TyKind::FnPtr(a), TyKind::FnPtr(b)) => {
            a.is_c == b.is_c && a.is_unsafe == b.is_unsafe && all(a.inputs_output, b.inputs_output)
        }
        (TyKind::Ref(m, a), TyKind::Ref(n, b)) | (TyKind::RawPtr(m, a), TyKind::RawPtr(n, b)) => {
            m == n && same_shape(a, b)
        }
        (TyKind::Array(a, n), TyKind::Array(b, m)) => n == m && same_shape(a, b),
        (TyKind::Slice(a), TyKind::Slice(b)) => same_shape(a, b),
        (TyKind::Tuple(a), TyKind::Tuple(b)) => all(a, b),
        (TyKind::Adt(a, x), TyKind::Adt(b, y)) => a == b && all(x, y),
        (TyKind::Closure(a, x), TyKind::Closure(b, y)) => a == b && all(x, y),
        (TyKind::FnDef(a), TyKind::FnDef(b)) => a.function == b.function && all(a.args, b.args),
        (TyKind::Alias(a), TyKind::Alias(b)) => {
            a.trait_ == b.trait_ && a.name == b.name && all(a.args, b.args)
        }
        _ => false,
    }
}
