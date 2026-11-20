//! Comparisons of function-pointer types by their region recipes.
use flux_hir::ItemTree;
use flux_ty_ir::fn_regions::instantiate;
use flux_ty_ir::{DynTy, FnPtrRegions, FnRegion, Ty, TyKind};

use crate::FnSigTy;

type BinderRows = Vec<Vec<Option<u32>>>;

fn pointer_binders(items: &ItemTree, sig: FnSigTy) -> Option<BinderRows> {
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
}

fn object_binders(items: &ItemTree, object: DynTy) -> Option<BinderRows> {
    if let Some(sig) = object.callable_signature() {
        return pointer_binders(items, sig);
    }
    // The compact Fn representation is used for independent elided input
    // regions. Generic output types are outside that input binder.
    crate::fn_trait_kind(items, object.trait_)?;
    let ([input], [output]) = (object.args, object.assoc_tys) else {
        return None;
    };
    let TyKind::Tuple(inputs) = input.kind() else {
        return None;
    };
    let mut ids = 0..;
    let mut rows: BinderRows = inputs
        .iter()
        .map(|&ty| {
            ids.by_ref()
                .take(crate::regions::slots(items, ty))
                .map(Some)
                .collect()
        })
        .collect();
    rows.push(vec![None; crate::regions::slots(items, *output)]);
    Some(rows)
}

/// Compare represented binders modulo alpha renaming. Missing or differently
/// shaped rows cannot prove a leak.
fn compare_rows(a: Option<BinderRows>, b: Option<BinderRows>) -> Option<bool> {
    let (a, b) = (a?, b?);
    if a.len() != b.len() || a.iter().zip(&b).any(|(a, b)| a.len() != b.len()) {
        return None;
    }
    let mut pairs = Vec::new();
    for (a, b) in a.into_iter().flatten().zip(b.into_iter().flatten()) {
        match (a, b) {
            (None, None) => {}
            (Some(a), Some(b)) => {
                if pairs.iter().any(|&(x, y)| (x == a) != (y == b)) {
                    return Some(false);
                }
                if !pairs.contains(&(a, b)) {
                    pairs.push((a, b));
                }
            }
            _ => return Some(false),
        }
    }
    Some(true)
}

/// Whether two intersected headers provably disagree in a callable binder.
/// Unlike selection, coherence can ask this with inference variables still
/// present in other positions, and needs a positive witness of a mismatch.
pub fn callable_binder_mismatch(items: &ItemTree, a: Ty, b: Ty) -> bool {
    fn visit(items: &ItemTree, a: Ty, b: Ty, depth: u32) -> bool {
        if a == b || depth >= 64 {
            return false;
        }
        match (a.kind(), b.kind()) {
            (TyKind::Dynamic(a), TyKind::Dynamic(b)) if a.trait_ == b.trait_ => {
                compare_rows(object_binders(items, a), object_binders(items, b)) == Some(false)
                    || a.args
                        .iter()
                        .chain(a.assoc_tys)
                        .zip(b.args.iter().chain(b.assoc_tys))
                        .any(|(&a, &b)| visit(items, a, b, depth + 1))
            }
            (TyKind::FnPtr(a), TyKind::FnPtr(b)) => {
                compare_rows(pointer_binders(items, a), pointer_binders(items, b)) == Some(false)
                    || a.inputs_output
                        .iter()
                        .zip(b.inputs_output)
                        .any(|(&a, &b)| visit(items, a, b, depth + 1))
            }
            (TyKind::Alias(_) | TyKind::Opaque(_), _)
            | (_, TyKind::Alias(_) | TyKind::Opaque(_)) => false,
            (left, right) if std::mem::discriminant(&left) == std::mem::discriminant(&right) => a
                .children()
                .zip(b.children())
                .any(|(a, b)| visit(items, a, b, depth + 1)),
            _ => false,
        }
    }
    visit(items, a, b, 0)
}

/// Equality of binder positions for trait/header matching. Early regions are
/// deliberately all external here; their equality is proven by the caller's
/// region tables. A bound region can never match an external region.
pub fn same_binders(items: &ItemTree, a: Ty, b: Ty) -> bool {
    if a == b || a.has_infer() || b.has_infer() {
        return true;
    }
    if let (TyKind::Dynamic(a), TyKind::Dynamic(b)) = (a.kind(), b.kind())
        && a.trait_ == b.trait_
        && crate::fn_trait_kind(items, a.trait_).is_some()
    {
        return compare_rows(object_binders(items, a), object_binders(items, b)) != Some(false)
            && a.args
                .iter()
                .chain(a.assoc_tys)
                .zip(b.args.iter().chain(b.assoc_tys))
                .all(|(&a, &b)| same_binders(items, a, b));
    }
    if let (TyKind::FnPtr(a), TyKind::FnPtr(b)) = (a.kind(), b.kind())
        && compare_rows(pointer_binders(items, a), pointer_binders(items, b)) != Some(true)
    {
        return false;
    }
    // Ordinary type matching/normalization owns all non-callable shapes.
    // This guard only adds binder identity at corresponding callable nodes.
    a.children()
        .zip(b.children())
        .all(|(a, b)| same_binders(items, a, b))
}

/// Substitution can leave references to concrete types that supply no regions.
/// Compare callable recipes after removing those empty contributions and
/// treating a fully empty recipe like the compact callable representation.
pub fn same_type_with_empty_callable_recipes(items: &ItemTree, a: Ty, b: Ty) -> bool {
    a == b || omit_empty_callable_regions(items, a) == omit_empty_callable_regions(items, b)
}

fn omit_empty_callable_regions(items: &ItemTree, ty: Ty) -> Ty {
    let ty = ty.map_children(|child| omit_empty_callable_regions(items, child));
    let TyKind::Dynamic(mut object) = ty.kind() else {
        return ty;
    };
    if crate::fn_trait_kind(items, object.trait_).is_some()
        && let Some(mut signature) = object.callable_signature()
        && signature.abi == flux_hir::Abi::Rust
        && !signature.is_unsafe
        && !signature.is_variadic
        && let [input] = object.args
        && let TyKind::Tuple(params) = input.kind()
        && params == signature.params()
        && object.assoc_tys == [signature.ret()]
        && signature.regions.is_none_or(|r| r.projections.is_empty())
    {
        let empty = |ty: Ty| {
            !ty.has_params()
                && !ty.has_infer()
                && !ty.has_aliases()
                && !ty.has_opaques()
                && !ty.has_error()
                && crate::regions::slots(items, ty) == 0
        };
        if let Some(regions) = signature.regions
            && regions.types.iter().copied().any(empty)
        {
            let mut types = Vec::new();
            let remap: Vec<_> = regions
                .types
                .iter()
                .map(|&ty| {
                    (!empty(ty)).then(|| {
                        let index = types.len() as u32;
                        types.push(ty);
                        index
                    })
                })
                .collect();
            let rows: Vec<_> = regions
                .inputs_output
                .iter()
                .map(|row| {
                    row.iter()
                        .filter_map(|&term| match term {
                            FnRegion::Type(i) => remap[i as usize].map(FnRegion::Type),
                            _ => Some(term),
                        })
                        .collect()
                })
                .collect();
            signature.regions = Some(FnPtrRegions::intern(
                regions.bound,
                regions.free,
                &rows,
                &types,
            ));
        }
        let empty_recipe = signature.regions.is_none_or(|r| {
            r.bound == 0
                && r.free == 0
                && r.types.is_empty()
                && r.inputs_output.iter().all(|row| row.is_empty())
        });
        object.callable = if empty_recipe && signature.inputs_output.iter().copied().all(empty) {
            None
        } else {
            Some(Ty::new(TyKind::FnPtr(signature)))
        };
        return Ty::new(TyKind::Dynamic(object));
    }
    ty
}

/// Type equality after erasing only function-pointer region recipes. This is
/// used before MIR checks callable subtyping, and for MIR storage validation.
pub fn same_shape(a: Ty, b: Ty) -> bool {
    same_shape_in(a, b, None)
}

/// MIR storage uses the same callable type when a substituted type contributes
/// a region row that was written explicitly at the call site. The borrow
/// checker proves the external regions at each use. Retain the bound-region
/// pattern and all of the metadata that determines object dispatch.
pub fn same_mir_shape(items: &ItemTree, a: Ty, b: Ty) -> bool {
    same_shape_in(a, b, Some(items))
}

fn same_shape_in(a: Ty, b: Ty, items: Option<&ItemTree>) -> bool {
    if a == b {
        return true;
    }
    let same = |a, b| same_shape_in(a, b, items);
    let all = |a: &[Ty], b: &[Ty]| a.len() == b.len() && a.iter().zip(b).all(|(&a, &b)| same(a, b));
    match (a.kind(), b.kind()) {
        (TyKind::FnPtr(a), TyKind::FnPtr(b)) => {
            a.abi == b.abi
                && a.is_variadic == b.is_variadic
                && a.is_unsafe == b.is_unsafe
                && all(a.inputs_output, b.inputs_output)
        }
        (TyKind::Ref(m, a), TyKind::Ref(n, b)) | (TyKind::RawPtr(m, a), TyKind::RawPtr(n, b)) => {
            m == n && same(a, b)
        }
        (TyKind::Array(a, n), TyKind::Array(b, m)) => n == m && same(a, b),
        (TyKind::Slice(a), TyKind::Slice(b)) => same(a, b),
        (TyKind::Tuple(a), TyKind::Tuple(b)) => all(a, b),
        (TyKind::Adt(a, x), TyKind::Adt(b, y)) => a == b && all(x, y),
        (TyKind::Closure(a, x), TyKind::Closure(b, y)) => a == b && all(x, y),
        (TyKind::FnDef(a), TyKind::FnDef(b)) => a.function == b.function && all(a.args, b.args),
        (TyKind::Alias(a), TyKind::Alias(b)) => {
            a.trait_ == b.trait_ && a.name == b.name && all(a.args, b.args)
        }
        (TyKind::Dynamic(a), TyKind::Dynamic(b)) if let Some(items) = items => {
            a.trait_ == b.trait_
                && crate::fn_trait_kind(items, a.trait_).is_some()
                && a.name == b.name
                && a.auto_traits == b.auto_traits
                && a.quantified == b.quantified
                && a.args == b.args
                && a.assoc_names == b.assoc_names
                && a.assoc_tys == b.assoc_tys
                && matches!((a.callable, b.callable), (Some(a), Some(b)) if same_shape(a, b))
                && compare_rows(object_binders(items, a), object_binders(items, b)) == Some(true)
        }
        _ => false,
    }
}
