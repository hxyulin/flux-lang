//! Ordinary builds discard test functions before collecting signatures and bodies.
use flux_hir::{CrateCfg, SourceDb, lower};
use flux_intern::Symbol;

#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn ordinary_test_attributes_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-test-attributes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepted) in [
        (
            "free",
            "#[test] fn hidden(x: Missing) { missing!(); }",
            true,
        ),
        ("generic", "#[test] fn hidden<T>() {}", true),
        ("unsafe", "#[test] unsafe fn hidden() {}", true),
        ("const", "#[test] const fn hidden() {}", true),
        ("async", "#[test] async fn hidden() {}", true),
        (
            "nested",
            "fn f() { #[test] fn hidden(x: Missing) { missing!(); } }",
            true,
        ),
        (
            "module",
            "mod m { #[test] pub fn hidden(x: Missing) {} }",
            true,
        ),
        ("same_name", "#[test] fn f() {} fn f() {}", true),
        (
            "conditional",
            "#[cfg_attr(all(), test)] fn hidden(x: Missing) {}",
            true,
        ),
        (
            "conditional_nested",
            "fn f(){ #[cfg_attr(all(), test)] fn hidden(x: Missing) {} }",
            true,
        ),
        (
            "conditional_off",
            "#[cfg_attr(any(), test)] fn kept() {}",
            true,
        ),
        (
            "cfg_removed_bad_attr",
            "#[cfg(any())] #[test(bad)] struct S;",
            true,
        ),
        (
            "macro_generated",
            "macro_rules! make { () => { #[test] fn hidden(x: Missing) { missing!(); } } } make!();",
            true,
        ),
        (
            "macro_generated_conditional",
            "macro_rules! make { () => { #[cfg_attr(all(), test)] fn hidden(x: Missing) {} } } make!();",
            true,
        ),
        (
            "body_type_macro",
            "#[test] fn hidden(x: missing!()) {}",
            true,
        ),
        (
            "body_local_macros",
            "#[test] fn hidden(){ macro_rules! missing {()=>{compile_error!(\"hidden\")}} missing!(); }",
            true,
        ),
        (
            "missing_name",
            "#[test] fn hidden() {} fn f(){ hidden(); }",
            false,
        ),
        (
            "missing_module_name",
            "mod m { #[test] pub fn hidden() {} } fn f(){m::hidden();}",
            false,
        ),
        (
            "missing_nested_name",
            "fn f(){#[test] fn hidden(){} hidden();}",
            false,
        ),
        (
            "foreign",
            "unsafe extern \"C\" { #[test] fn hidden(); }",
            false,
        ),
        ("field", "struct S { #[test] x:u32 }", false),
        ("parameter", "fn f(#[test] x:u32) {}", false),
        ("variant", "enum E { #[test] V }", false),
        ("struct", "#[test] struct S;", false),
        ("module_bad", "#[test] mod m {}", false),
        ("const_bad", "#[test] const N:u32=0;", false),
        (
            "impl_method",
            "struct S; impl S { #[test] fn f(&self) {} }",
            false,
        ),
        (
            "impl_static",
            "struct S; impl S { #[test] fn f() {} }",
            false,
        ),
        ("trait_method", "trait T { #[test] fn f(); }", false),
        (
            "conditional_impl",
            "struct S; impl S { #[cfg_attr(all(), test)] fn f() {} }",
            false,
        ),
        (
            "impl_macro",
            "macro_rules! make {()=>{ #[test] fn f() {} }} struct S; impl S { make!(); }",
            false,
        ),
        ("nested_struct", "fn f(){#[test] struct S;}", false),
        ("payload", "#[test(bad)] fn hidden() {}", false),
        ("value", "#[test=\"bad\"] fn hidden() {}", false),
        ("unsafe_attr", "#[unsafe(test)] fn hidden() {}", false),
        (
            "conditional_payload",
            "#[cfg_attr(all(), test(bad))] fn hidden() {}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        for configured_test in [false, true] {
            let path = dir.join(format!("{name}.rs"));
            std::fs::write(&path, source).unwrap();
            let mut command = std::process::Command::new("rustc");
            command.args(["--edition=2024", "--crate-type=lib", "--emit=metadata"]);
            if configured_test {
                command.args(["--cfg", "test"]);
            }
            let output = command
                .arg(&path)
                .arg("-o")
                .arg(dir.join("probe.rmeta"))
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                accepted,
                "{name}, cfg(test)={configured_test}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let mut db = SourceDb::new();
            let root = db.add(&path, source);
            if configured_test {
                db.set_crate_cfg(
                    root,
                    CrateCfg {
                        names: vec![Symbol::intern("test")],
                        values: vec![],
                    },
                )
                .unwrap();
            }
            let hir = lower(&mut db, root, Symbol::intern("test_attributes"));
            assert!(db.file(root).parse.errors().is_empty(), "{name}");
            assert_eq!(
                hir.diagnostics.is_empty(),
                accepted,
                "{name}, cfg(test)={configured_test}: {:?}",
                hir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
