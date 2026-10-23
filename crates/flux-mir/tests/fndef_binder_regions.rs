//! A callback binder cannot generalize a function item's fixed owner regions.
use std::process::Command;

const BOUNDARY: &str = "higher-ranked callbacks with owner lifetimes are not supported yet";

#[test]
fn fixed_owner_callback_binders_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-fndef-binders-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let owner = "struct S<'a>(&'a i32); impl<'a> S<'a> {";
    let mut cases = Vec::new();
    // No output relation is needed to expose the missing generality proof.
    for kind in ["Fn", "FnMut", "FnOnce"] {
        for (ret, body) in [("&'a i32", "x"), ("i32", "*x"), ("()", "()")] {
            cases.push((
                format!("negative_{kind}_{}", cases.len()),
                format!("{owner} fn get(x: &'a i32) -> {ret} {{ {body} }} }} fn need<F: for<'a> {kind}(&'a i32) -> {ret}>(_: F) {{}} fn f() {{ need(S::get); }} fn main() {{}}"),
                false,
            ));
        }
    }
    cases.push(("negative_elided".into(), format!("{owner} fn get(x: &'a i32) -> &'a i32 {{ x }} }} fn need<F: Fn(&i32) -> &i32>(_: F) {{}} fn f() {{ need(S::get); }} fn main() {{}}"), false));
    cases.push(("negative_mixed".into(), format!("{owner} fn get(_: i32, x: &'a i32) -> i32 {{ *x }} }} fn need<T, F: Fn(T, &i32) -> i32>(_: T, _: F) {{}} fn f() {{ need(0, S::get); }} fn main() {{}}"), false));
    cases.push(("positive_free_identity".into(), "fn get(x: &i32) -> &i32 { x } fn need<F: for<'a> Fn(&'a i32) -> &'a i32>(_: F) {} fn main() { need(get); }".into(), true));
    cases.push(("positive_type_slots".into(), format!("{owner} fn get(x: &'a i32) -> &'a i32 {{ x }} }} fn need<T, F: Fn(T) -> T>(_: T, _: F) {{}} fn main() {{ let x = 7; need(&x, S::get); }}"), true));
    cases.push(("positive_find_map".into(), format!("{owner} fn get(x: &'a i32) -> Option<&'a i32> {{ Some(x) }} }} fn main() {{ let x = 7; assert_eq!(*[&x].into_iter().find_map(S::get).unwrap(), 7); }}"), true));
    cases.push(("positive_generic_owner".into(), "struct S<T>(T); impl<T> S<T> { fn get(x: &T) -> &T { x } } fn need<F: for<'a> Fn(&'a i32) -> &'a i32>(_: F) {} fn main() { need(S::get); }".into(), true));
    // This deliberately bounded check also rejects valid owner-bearing
    // items whose inputs use independent late lifetimes. Supporting them
    // requires a binder-aware proof, rather than fresh owner inference.
    cases.push(("unsupported_unused_owner".into(), format!("{owner} fn get(x: &i32) -> &i32 {{ x }} }} fn need<F: Fn(&i32) -> &i32>(_: F) {{}} fn main() {{ need(S::get); }}"), true));
    cases.push(("negative_trait_owner_gated".into(), "trait Tr<'a> { fn get(x: &'a i32) -> &'a i32; } struct S; impl<'a> Tr<'a> for S { fn get(x: &'a i32) -> &'a i32 { x } } fn need<F: for<'a> Fn(&'a i32) -> &'a i32>(_: F) {} fn f() { need(S::get); } fn main() {}".into(), false));
    for (name, source, accepts) in cases {
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
            accepts,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        if !accepts {
            assert!(
                String::from_utf8_lossy(&rustc.stderr).contains("not general enough"),
                "{name}: {}",
                String::from_utf8_lossy(&rustc.stderr)
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
        if name == "negative_trait_owner_gated" {
            assert!(
                !typed.diagnostics.is_empty(),
                "declaration gate must stay closed"
            );
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .all(|d| d.message == "lifetime parameters on traits are not supported yet"),
                "{name}: {:?}",
                typed.diagnostics
            );
        } else {
            assert!(
                typed.diagnostics.is_empty(),
                "{name}: {:?}",
                typed.diagnostics
            );
        }
        let mir = flux_mir::build_file(&hir, &typed);
        if accepts && !name.starts_with("unsupported_") {
            assert!(mir.diagnostics.is_empty(), "{name}: {:?}", mir.diagnostics);
        } else {
            assert!(!mir.diagnostics.is_empty(), "{name}");
            assert!(
                mir.diagnostics.iter().all(|d| d.message == BOUNDARY),
                "{name}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
