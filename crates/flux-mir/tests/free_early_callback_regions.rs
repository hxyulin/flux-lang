//! Explicit signature bounds are not proof of a universal callback binder.
use std::process::Command;

const BOUNDARY: &str =
    "higher-ranked callbacks with signature lifetime bounds are not supported yet";

#[test]
fn free_early_callback_binders_match_rustc() {
    let mut cases = Vec::new();
    for kind in ["Fn", "FnMut", "FnOnce"] {
        for (suffix, ret, body) in [
            ("ref", "&'a i32", "x"),
            ("scalar", "i32", "*x"),
            ("unit", "()", "()"),
        ] {
            for (bound, generics, clause) in [
                ("inline", "'a: 'a", ""),
                ("where", "'a", "where 'a: 'a"),
                ("type", "'a, T: 'a", ""),
            ] {
                let input = if bound == "type" { "T" } else { "i32" };
                let body = if bound == "type" && suffix == "scalar" {
                    "17"
                } else {
                    body
                };
                let ret = if bound == "type" && suffix == "ref" {
                    "&'a T"
                } else {
                    ret
                };
                cases.push((format!("{kind}_{suffix}_{bound}_bad"), format!("fn get<{generics}>(x: &'a {input}) -> {ret} {clause} {{ {body} }} fn need<F: {kind}(&i32) -> {}>(_: F) {{}} fn main() {{ need(get); }}", if suffix == "ref" { "&i32" } else { ret }), false, true));
            }
        }
    }
    for (name, source) in [
        (
            "named_late",
            "fn get<'a>(x: &'a i32) -> &'a i32 { x } fn need<F: Fn(&i32) -> &i32>(_:F) {} fn main(){need(get);}",
        ),
        (
            "elided_late",
            "fn get(x: &i32) -> &i32 { x } fn need<F: Fn(&i32) -> &i32>(_:F) {} fn main(){need(get);}",
        ),
        (
            "fixed_param",
            "fn get<'a:'a>(x:&'a i32)->&'a i32{x} fn need<T,F:Fn(T)->T>(x:T,f:F)->T{f(x)} fn main(){let x=17;assert_eq!(*need(&x,get),17);}",
        ),
        (
            "fixed_alias",
            "fn get<'a:'a>(x:&'a i32)->Option<&'a i32>{Some(x)} fn main(){let x=17;assert_eq!(*[&x].into_iter().find_map(get).unwrap(),17);}",
        ),
        (
            "fixed_unit",
            "fn get<'a:'a>(_:&'a i32){} fn need<T,F:Fn(T)>(x:T,f:F){f(x)} fn main(){let x=17;need(&x,get);}",
        ),
        (
            "fixed_scalar",
            "fn get<'a:'a>(x:&'a i32)->i32{*x} fn need<T,F:Fn(T)->i32>(x:T,f:F)->i32{f(x)} fn main(){let x=17;assert_eq!(need(&x,get),17);}",
        ),
    ] {
        cases.push((name.into(), source.into(), true, false));
    }
    // has_bounds is intentionally a conservative signature-level boundary,
    // not an early/late classifier. Both of these inputs are late in Rust.
    for (name, source) in [
        (
            "unrelated_lifetime",
            "fn get<'u:'u,'a>(x:&'a i32)->&'a i32{x} fn need<F:Fn(&i32)->&i32>(_:F){} fn main(){need(get);}",
        ),
        (
            "unrelated_type",
            "fn get<'a,T:'static>(x:&'a T)->&'a T{x} fn need<F:Fn(&i32)->&i32>(_:F){} fn main(){need(get);}",
        ),
    ] {
        cases.push((name.into(), source.into(), true, true));
    }
    // A fixed Param slot must not hide a second, independently bound input.
    cases.push(("mixed_bad".into(), "fn get<'a:'a>(_:i32,x:&'a i32)->i32{*x} fn need<T,F:Fn(T,&i32)->i32>(_:T,_:F){} fn main(){need(0,get);}".into(), false, true));
    let dir = std::env::temp_dir().join(format!("flux-free-early-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, rust_valid, rejects) in cases {
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = Command::new("rustc")
            .args([
                "--edition=2024",
                "--emit=metadata",
                "-Awarnings",
                "--out-dir",
            ])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&rustc.stderr);
        assert_eq!(rustc.status.success(), rust_valid, "{name}: {stderr}");
        if !rust_valid {
            assert!(stderr.contains("not general enough"), "{name}: {stderr}");
        }
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{name}: {:?}",
            typed.diagnostics
        );
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            !mir.diagnostics.is_empty(),
            rejects,
            "{name}: {:?}",
            mir.diagnostics
        );
        if rejects {
            assert!(
                mir.diagnostics.iter().all(|d| d.message == BOUNDARY),
                "{name}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_free_callback_metadata_fails_closed() {
    let source = "fn get<'a:'a>(_:&'a i32){} fn need<F:Fn(&i32)>(_:F){} fn main(){need(get);}";
    let (_, _, hir) = flux_hir::lower_text(std::path::Path::new("metadata.rs"), source);
    let get = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "get" && f.owner.is_none())
        .unwrap()
        .0;
    let need = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "need" && f.owner.is_none())
        .unwrap()
        .0;
    for corruption in 0..5 {
        let mut typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty());
        match corruption {
            0 => {
                let old = std::mem::take(&mut typed.sigs.regions.fns);
                for (id, regions) in old.iter().filter(|(id, _)| *id != get) {
                    typed.sigs.regions.fns.insert(id, regions.clone());
                }
            }
            1 => {
                typed.sigs.regions.fns.get_mut(need).unwrap().bound_traits[0].args = Box::default()
            }
            2 => {
                typed.sigs.regions.fns.get_mut(need).unwrap().bound_traits[0].args[0] =
                    Box::default()
            }
            3 => {
                typed.sigs.regions.fns.get_mut(need).unwrap().bound_traits[0].args[0][0] =
                    flux_ty::regions::Region::Erased
            }
            _ => {
                typed.sigs.regions.fns.get_mut(need).unwrap().bound_traits[0].args[0][0] =
                    flux_ty::regions::Region::Param(999)
            }
        }
        let mir = flux_mir::build_file(&hir, &typed);
        let expected = if corruption == 0 {
            "function item callback region metadata is not supported yet"
        } else {
            BOUNDARY
        };
        assert!(
            mir.diagnostics.iter().any(|d| d.message == expected),
            "{corruption}: {:?}",
            mir.diagnostics
        );
    }
}
