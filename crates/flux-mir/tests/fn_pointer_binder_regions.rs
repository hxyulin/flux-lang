//! Reification must not erase signature-bound lifetimes before MIR sees them.
use std::process::Command;

const BOUNDARY: &str =
    "function pointers to functions with constrained input lifetimes are not supported yet";
const METADATA: &str = "function item pointer region metadata is not supported yet";

#[test]
fn pointer_coercion_paths_match_rustc() {
    let mut cases = Vec::new();
    for (predicate, prelude, clause) in [
        ("copy", "", "&'a i32: Copy"),
        ("erased_alias", "type E<'a> = i32;", "E<'a>: Copy"),
        (
            "equality",
            "trait Tr {type Out;} impl Tr for () {type Out=&'static i32;}",
            "():Tr<Out=&'a i32>",
        ),
    ] {
        for (path, usage) in [
            ("annotation", "let _:fn(&i32)=get;"),
            ("cast", "let _=get as fn(&i32);"),
            ("argument", "need(get);"),
            ("clone", "let _:fn(&i32)=get.clone();"),
            ("pointer_alias", "let _:P=get;"),
            ("unsafe_pointer", "let _:unsafe fn(&i32)=get;"),
            ("field", "let _=S{f:get};"),
            ("return", "fn pointer()->fn(&i32){get}"),
        ] {
            cases.push((format!("{predicate}_{path}"), format!("{prelude} fn get<'a>(_:&'a i32) where {clause} {{}} type P=fn(&i32); struct S{{f:P}} fn need(_:P){{}} fn main(){{{usage}}}"), false, Some(BOUNDARY)));
        }
    }
    for (name, source, valid, diagnostic) in [
        (
            "static",
            "fn get(_: &'static i32){} fn main(){let _:fn(&i32)=get;}",
            false,
            Some(BOUNDARY),
        ),
        (
            "owner",
            "struct S<'a>(&'a i32); impl<'a>S<'a>{fn get(_: &'a i32){}} fn main(){let _:fn(&i32)=S::get;}",
            false,
            Some(BOUNDARY),
        ),
        // Rust accepts repeated late inputs; Flux deliberately retains its narrower boundary.
        (
            "repeated",
            "fn get<'a>(_: &'a i32,_: &'a i32){} fn main(){let _:fn(&i32,&i32)=get;}",
            true,
            Some(BOUNDARY),
        ),
        (
            "outlives",
            "fn get<'a:'a>(_: &'a i32){} fn main(){let _:fn(&i32)=get;}",
            false,
            Some(BOUNDARY),
        ),
        (
            "unrelated_outlives",
            "fn get<'a,T:'static>(_: &'a T){} fn main(){let _:fn(&i32)=get;}",
            true,
            Some(BOUNDARY),
        ),
        (
            "unclassified",
            "fn get<'a>(_: &'a i32) where [i32;1+1]:Copy{} fn main(){let _:fn(&i32)=get;}",
            true,
            Some(METADATA),
        ),
        (
            "named",
            "fn get<'a>(x:&'a i32)->&'a i32{x} fn main(){let _:fn(&i32)->&i32=get;}",
            true,
            None,
        ),
        (
            "elided_clone",
            "fn get(x:&i32)->&i32{x} fn main(){let _:fn(&i32)->&i32=get.clone();}",
            true,
            None,
        ),
        (
            "unrelated_copy",
            "fn get<'a,T:Copy>(x:&'a T)->&'a T{x} fn main(){let _:fn(&i32)->&i32=get;}",
            true,
            None,
        ),
        (
            "static_predicate",
            "fn get<'a>(x:&'a i32)->&'a i32 where &'static i32:Copy{x} fn main(){let _:fn(&i32)->&i32=get;}",
            true,
            None,
        ),
        (
            "generic",
            "fn get<T>(x:T)->T{x} fn main(){let _:fn(i32)->i32=get;}",
            true,
            None,
        ),
        (
            "safe_to_unsafe",
            "fn get(_: &i32){} fn main(){let _:unsafe fn(&i32)=get;}",
            true,
            None,
        ),
        (
            "unsafe",
            "unsafe fn get(_: &i32){} fn main(){let _:unsafe fn(&i32)=get;}",
            true,
            None,
        ),
        (
            "unsafe_to_safe",
            "unsafe fn get(_: &i32){} fn main(){let _:fn(&i32)=get;}",
            false,
            Some("mismatched types"),
        ),
        (
            "unsafe_bound",
            "unsafe fn get<'a>(_: &'a i32) where &'a i32:Copy{} fn main(){let _:unsafe fn(&i32)=get;}",
            false,
            Some(BOUNDARY),
        ),
    ] {
        cases.push((name.into(), source.into(), valid, diagnostic));
    }
    let dir = std::env::temp_dir().join(format!("flux-pointer-binder-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, rust_valid, diagnostic) in cases {
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
        assert_eq!(
            rustc.status.success(),
            rust_valid,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        if !rust_valid {
            assert!(
                String::from_utf8_lossy(&rustc.stderr).contains("E0308")
                    || String::from_utf8_lossy(&rustc.stderr).contains("E0605"),
                "{name}: {rustc:?}"
            );
        }
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        match diagnostic {
            Some(expected) => {
                assert!(!typed.diagnostics.is_empty(), "{name}");
                assert!(
                    typed
                        .diagnostics
                        .iter()
                        .all(|d| d.message.starts_with(expected)),
                    "{name}: {:?}",
                    typed.diagnostics
                );
                // Rejected reifications must never reach code generation.
            }
            None => {
                assert!(
                    typed.diagnostics.is_empty(),
                    "{name}: {:?}",
                    typed.diagnostics
                );
                let mir = flux_mir::build_file(&hir, &typed);
                assert!(mir.diagnostics.is_empty(), "{name}: {:?}", mir.diagnostics);
            }
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_or_unclassified_pointer_metadata_fails_closed() {
    for usage in [
        "let _:fn(&i32)=get;",
        "let _=get as fn(&i32);",
        "need(get);",
        "let _:fn(&i32)=get.clone();",
        "let _:P=get;",
    ] {
        let source = format!(
            "fn get<'a>(_:&'a i32){{}} type P=fn(&i32); fn need(_:P){{}} fn main(){{{usage}}}"
        );
        let (db, _, hir) = flux_hir::lower_text("metadata.rs", &source);
        assert!(db.syntax_errors().is_empty());
        assert!(hir.diagnostics.is_empty());
        let (sigs, diagnostics) = flux_ty::signatures(&hir.items);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let id = |name: &str| {
            hir.items
                .functions
                .iter()
                .find(|(_, f)| f.name.as_str() == name && f.owner.is_none())
                .unwrap()
                .0
        };
        let get = id("get");
        let main = id("main");
        let (body, map) = &hir.bodies[main];
        for missing in [false, true] {
            let mut changed = sigs.clone();
            if missing {
                let old = std::mem::take(&mut changed.regions.fns);
                for (id, r) in old.iter().filter(|(id, _)| *id != get) {
                    changed.regions.fns.insert(id, r.clone());
                }
            } else {
                changed.regions.fns.get_mut(get).unwrap().callback_binder = Default::default();
            }
            let inferred = flux_ty::infer_body(&hir.items, &changed, main, body, map);
            assert!(
                !inferred.diagnostics.is_empty(),
                "{usage}, missing={missing}"
            );
            assert!(
                inferred.diagnostics.iter().all(|d| d.message == METADATA),
                "{usage}, missing={missing}: {:?}",
                inferred.diagnostics
            );
            assert!(inferred.reified.iter().next().is_none(), "{usage}");
        }
        let inferred = flux_ty::infer_body(&hir.items, &sigs, main, body, map);
        assert!(
            inferred.diagnostics.is_empty(),
            "{usage}: {:?}",
            inferred.diagnostics
        );
        assert!(inferred.reified.iter().next().is_some(), "{usage}");
    }
}
