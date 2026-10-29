//! Validate trusted sized/unsized helper pairs before monomorphization.
use crate::{GenericArgs, Metadata, Signatures, regions::Outlives};
use flux_hir::{Diagnostic, FunctionId, Intrinsic, ItemTree, SizedDispatch};

fn helpers_match(
    items: &ItemTree,
    sigs: &Signatures,
    carrier: FunctionId,
    helper: FunctionId,
    selector: usize,
    sized: bool,
) -> bool {
    let from = &items.functions[carrier];
    let to = &items.functions[helper];
    if to.owner.is_some()
        || to.intrinsic.is_some()
        || !to.has_body
        || to.is_const
        || to.is_unsafe
        || to.c_abi
        || to.is_extern
        || to.is_variadic
        || to.track_caller
        || to.target_features.is_some()
        || !to.opaques.is_empty()
        || items.modules[to.loc.module].krate != items.modules[from.loc.module].krate
        || from.generics.params.len() != to.generics.params.len()
        || from.generics.lifetimes.len() != to.generics.lifetimes.len()
    {
        return false;
    }
    for (index, (a, b)) in from
        .generics
        .params
        .iter()
        .zip(&to.generics.params)
        .enumerate()
    {
        if a.is_const() != b.is_const()
            || crate::param_const_ty(a) != crate::param_const_ty(b)
            || a.synthetic != b.synthetic
            || a.may_dangle
            || b.may_dangle
            || a.default.is_some()
            || b.default.is_some()
            || (if index == selector {
                b.maybe_unsized == sized
            } else {
                a.maybe_unsized != b.maybe_unsized
            })
        {
            return false;
        }
    }
    let args = crate::params_of(&from.generics, 0);
    let Some(expected_types) = sigs.fns.get(carrier) else {
        return false;
    };
    if expected_types.params.iter().any(|ty| ty.has_error())
        || expected_types.ret.has_error()
        || sigs.fns.get(helper).map(|sig| sig.subst(&args)) != Some(expected_types.clone())
    {
        return false;
    }
    let base = sigs.fn_predicates.get(carrier).map_or(&[][..], |p| p);
    let predicates = sigs.fn_predicates.get(helper).map_or(&[][..], |p| p);
    let keep: Vec<_> = predicates
        .iter()
        .enumerate()
        .filter_map(|(index, p)| {
            let p = p.subst(&args);
            let extra_sized = sized
                && Some(p.trait_ref.trait_) == sigs.builtins.sized
                && p.self_ty == args[selector]
                && p.trait_ref.args.is_empty();
            (!extra_sized).then_some(index)
        })
        .collect();
    if keep
        .iter()
        .map(|&i| predicates[i].subst(&args))
        .collect::<Vec<_>>()
        != base
    {
        return false;
    }
    let projections = sigs.fn_projections.get(helper).map_or(&[][..], |p| p);
    if projections
        .iter()
        .map(|p| p.subst(&args))
        .collect::<Vec<_>>()
        != sigs.fn_projections.get(carrier).map_or(&[][..], |p| p)
    {
        return false;
    }
    let Some(mut regions) = sigs.regions.fns.get(helper).cloned() else {
        return false;
    };
    let Some(expected) = sigs.regions.fns.get(carrier) else {
        return false;
    };
    if regions.names.len() != expected.names.len() || regions.owner != 0 || expected.owner != 0 {
        return false;
    }
    // Regions use relative parameter slots. Names/ranges are declaration-local;
    // the complete recipes, links, predicates and bounds must remain identical.
    regions.names = expected.names.clone();
    regions.outlives = regions
        .outlives
        .iter()
        .map(|bound| match *bound {
            Outlives::Type(ty, region) => Outlives::Type(ty.subst(&args), region),
            other => other,
        })
        .collect();
    if regions.bound_selfs.len() != predicates.len()
        || regions.bound_traits.len() != predicates.len()
    {
        return false;
    }
    regions.bound_selfs = keep
        .iter()
        .map(|&i| regions.bound_selfs[i].clone())
        .collect();
    regions.bound_traits = keep
        .iter()
        .map(|&i| regions.bound_traits[i].clone())
        .collect();
    regions == *expected
}

pub(crate) fn validate(items: &ItemTree, sigs: &mut Signatures, errors: &mut Vec<Diagnostic>) {
    for (&carrier, &dispatch) in &items.sized_dispatch {
        let data = &items.functions[carrier];
        let selector = dispatch.selector as usize;
        let valid = data.intrinsic == Some(Intrinsic::SizedDispatch)
            && (items.is_core(data.loc.module) || items.is_std(data.loc.module))
            && dispatch.parameters as usize == data.generics.params.len()
            && data.owner.is_none()
            && !data.is_const
            && !data.is_unsafe
            && !data.c_abi
            && !data.is_extern
            && !data.is_variadic
            && !data.track_caller
            && data.target_features.is_none()
            && data.opaques.is_empty()
            && data
                .generics
                .params
                .get(selector)
                .is_some_and(|p| !p.is_const() && p.maybe_unsized)
            && helpers_match(items, sigs, carrier, dispatch.sized, selector, true)
            && helpers_match(items, sigs, carrier, dispatch.fallback, selector, false);
        if valid {
            sigs.sized_dispatch.insert(carrier, dispatch);
        } else {
            errors.push(Diagnostic { file: data.loc.file, range: data.sig_range,
            message: "sized dispatch helpers must match the carrier except for the selector Sized bound".into() });
        }
    }
}

/// Only concrete metadata may select a body. This is also used by mono's
/// collector, before the chosen helper's instance and ABI are registered.
pub fn select(
    sigs: &Signatures,
    carrier: FunctionId,
    args: GenericArgs,
) -> Result<FunctionId, &'static str> {
    let Some(SizedDispatch {
        selector,
        parameters,
        sized,
        fallback,
    }) = sigs.sized_dispatch.get(&carrier).copied()
    else {
        return Err("sized dispatch has no validated helper metadata");
    };
    if args.len() != parameters as usize {
        return Err("sized dispatch generic argument count does not match");
    }
    let Some(&ty) = args.get(selector as usize) else {
        return Err("sized dispatch selector is missing");
    };
    if matches!(ty.kind(), crate::TyKind::Value(_)) {
        return Err("sized dispatch selector must be a type");
    }
    if ty.has_error() || ty.has_params() || ty.has_infer() || ty.has_aliases() || ty.has_opaques() {
        return Err("sized dispatch selector must be fully resolved");
    }
    match sigs.metadata(ty) {
        Metadata::Thin => Ok(sized),
        Metadata::Length | Metadata::VTable => Ok(fallback),
        Metadata::Generic(_) => Err("sized dispatch selector metadata must be resolved"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prepare(source: &str) -> (flux_hir::Hir, Signatures, FunctionId) {
        let (_, _, mut hir) = flux_hir::lower_text("dispatch.rs", source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let (sigs, errors) = crate::signatures(&hir.items);
        assert!(errors.is_empty(), "{errors:?}");
        let find = |name: &str| {
            hir.items
                .functions
                .iter()
                .find(|(_, f)| {
                    f.name.as_str() == name
                        && hir.items.modules[f.loc.module].krate == hir.items.main_crate()
                })
                .unwrap()
                .0
        };
        let carrier = find("carrier");
        let dispatch = SizedDispatch {
            selector: 0,
            parameters: hir.items.functions[carrier].generics.params.len() as u32,
            sized: find("sized"),
            fallback: find("fallback"),
        };
        // Unit-only metadata models an embedded declaration. Separate public
        // source probes reject both reserved attributes and fake root names.
        let krate = hir.items.modules[hir.items.functions[carrier].loc.module].krate;
        hir.items.crates[krate.0 as usize].is_core = true;
        hir.items.functions[carrier].intrinsic = Some(Intrinsic::SizedDispatch);
        hir.items.sized_dispatch.insert(carrier, dispatch);
        (hir, sigs, carrier)
    }
    #[test]
    fn paired_headers_preserve_generics_regions_and_bounds() {
        let base =
            "fn carrier<T:?Sized>(value:&T)->&T{value} fn fallback<U:?Sized>(value:&U)->&U{value} ";
        for (helper, accepted) in [
            ("fn sized<V>(value:&V)->&V{value}", true),
            ("fn sized<V:Sized>(value:&V)->&V{value}", true),
            ("fn sized<V:?Sized>(value:&V)->&V{value}", false),
            ("fn sized<V:Clone>(value:&V)->&V{value}", false),
            ("unsafe fn sized<V>(value:&V)->&V{value}", false),
            ("const fn sized<V>(value:&V)->&V{value}", false),
            ("fn sized<V>(value:&V)->(){let _=value;}", false),
            ("fn sized<V,W>(value:&V)->&V{value}", false),
        ] {
            let (hir, mut sigs, carrier) = prepare(&format!("{base}{helper}"));
            let mut errors = Vec::new();
            validate(&hir.items, &mut sigs, &mut errors);
            assert_eq!(errors.is_empty(), accepted, "{helper}: {errors:?}");
            assert_eq!(sigs.sized_dispatch.contains_key(&carrier), accepted);
        }
        for (source, accepted) in [
            (
                "fn carrier<'a,T:?Sized,U,const N:usize>(a:&'a T,b:[U;N])->(&'a T,[U;N]){(a,b)} fn sized<'b,V,W,const M:usize>(a:&'b V,b:[W;M])->(&'b V,[W;M]){(a,b)} fn fallback<'c,X:?Sized,Y,const L:usize>(a:&'c X,b:[Y;L])->(&'c X,[Y;L]){(a,b)}",
                true,
            ),
            (
                "fn carrier<'a,'b,T:?Sized>(a:&'a T,b:&'b T)->&'a T{a}fn sized<'x,'y,V>(a:&'x V,b:&'y V)->&'y V{b}fn fallback<'x,'y,V:?Sized>(a:&'x V,b:&'y V)->&'x V{a}",
                false,
            ),
            (
                "fn carrier<T:?Sized,U:?Sized>(a:&T,b:&U){}fn sized<T,U>(a:&T,b:&U){}fn fallback<T:?Sized,U:?Sized>(a:&T,b:&U){}",
                false,
            ),
            (
                "fn carrier<T:?Sized,const N:usize>(a:&T){}fn sized<T,const N:u32>(a:&T){}fn fallback<T:?Sized,const N:usize>(a:&T){}",
                false,
            ),
            (
                "fn carrier<T:?Sized>(a:&T){}fn sized<T>(a:&T){}fn fallback<T>(a:&T){}",
                false,
            ),
        ] {
            let (hir, mut sigs, _) = prepare(source);
            let mut errors = Vec::new();
            validate(&hir.items, &mut sigs, &mut errors);
            assert_eq!(errors.is_empty(), accepted, "{source}: {errors:?}");
        }
    }
    #[test]
    fn selectors_require_resolved_concrete_metadata() {
        let (hir, mut sigs, carrier) = prepare(
            "fn carrier<T:?Sized>(v:&T)->&T{v}fn sized<T>(v:&T)->&T{v}fn fallback<T:?Sized>(v:&T)->&T{v}",
        );
        let mut errors = Vec::new();
        validate(&hir.items, &mut sigs, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        let dispatch = sigs.sized_dispatch[&carrier];
        assert_eq!(
            select(
                &sigs,
                carrier,
                crate::Ty::list(&[crate::Ty::int(crate::IntTy::U32)])
            ),
            Ok(dispatch.sized)
        );
        assert_eq!(
            select(&sigs, carrier, crate::Ty::list(&[crate::Ty::STR])),
            Ok(dispatch.fallback)
        );
        assert!(select(&sigs, carrier, &[]).is_err());
        assert!(
            select(
                &sigs,
                carrier,
                crate::Ty::list(&[crate::Ty::STR, crate::Ty::STR])
            )
            .is_err()
        );
        assert!(
            select(
                &sigs,
                carrier,
                crate::Ty::list(&[crate::Ty::param(0, flux_intern::Symbol::intern("T"))])
            )
            .is_err()
        );
        let mut missing = sigs.clone();
        missing.sized_dispatch.remove(&carrier);
        assert!(select(&missing, carrier, &[]).is_err());
    }
    #[test]
    fn intrinsic_flags_keep_pointer_and_const_restrictions() {
        for (use_site, expected) in [
            (
                "fn main(){let _:fn(&u32)->&u32=carrier::<u32>;}",
                "intrinsic",
            ),
            ("const fn f(v:&u32)->&u32{carrier(v)}fn main(){}", "const"),
        ] {
            let source = format!("fn carrier<T:?Sized>(v:&T)->&T{{v}}{use_site}");
            let (_, _, mut hir) = flux_hir::lower_text("intrinsic-use.rs", &source);
            assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
            let carrier = hir
                .items
                .functions
                .iter()
                .find(|(_, f)| f.name.as_str() == "carrier")
                .unwrap()
                .0;
            // A unit-only declaration flag exercises the same existing infer
            // guards used for every authenticated intrinsic operation.
            hir.items.functions[carrier].intrinsic = Some(Intrinsic::SizedDispatch);
            let typed = crate::check_file(&hir);
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains(expected)),
                "{:?}",
                typed.diagnostics
            );
        }
    }
}
