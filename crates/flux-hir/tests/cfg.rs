//! Conditional compilation must run before collecting names and bodies.
use flux_hir::{SourceDb, lower};
use flux_intern::Symbol;

#[test]
fn file_module_inner_cfg_removes_its_binding_and_macros() {
    let mut db = SourceDb::new();
    let dir = std::env::temp_dir().join(format!("flux-cfg-modules-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hidden.rs"), "#![cfg_attr(all(), cfg(any()))]\ncompile_error!(\"disabled\");\nmacro_rules! escaped { () => { 1 }; }").unwrap();
    let root = db.add(
        dir.join("main.rs"),
        "#[macro_use] mod hidden; fn main() { hidden::missing(); escaped!(); }",
    );
    let hir = lower(&mut db, root, Symbol::intern("configured"));
    std::fs::remove_dir_all(dir).unwrap();
    assert!(
        hir.items
            .modules
            .iter()
            .all(|(_, m)| m.name.as_str() != "hidden")
    );
    let messages: Vec<_> = hir.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "cannot find module or crate `hidden` in this scope",
            "cannot find macro `escaped` in this scope"
        ]
    );
}

#[test]
fn cfg_attr_does_not_hide_unsupported_conditional_positions() {
    for source in [
        "struct S { #[cfg_attr(all(), cfg(any()))] f: i32 }",
        "enum E { #[cfg_attr(all(), cfg(any()))] V }",
        "fn f(#[cfg_attr(all(), cfg(any()))] p: i32) {}",
        "fn f() { let x = 1 + #[cfg_attr(all(), cfg(any()))] 2; }",
        "enum E { #[cfg_attr(not(), allow(dead_code))] V }",
        "fn f() { if true { #![cfg(any())] } }",
        "fn f() { let x = #[cfg_attr(all(), cfg(any()))] { 1 }; }",
        "fn f() { let x = [#[cfg(any())] 1; 3]; }",
        "#[unsafe(cfg_attr(all(), inline))] fn f() {}",
        "#[unsafe(macro_use)] mod macros {}",
    ] {
        let (_, _, hir) = flux_hir::lower_text("cfg.rs", source);
        assert!(!hir.diagnostics.is_empty(), "accepted {source}");
    }
}
