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

#[test]
fn conditional_members_match_rustc() {
    let accepted = [
        include_str!("data/ok/cfg_members.rs"),
        "struct S { #[cfg_attr(all(), cfg_attr(all(), cfg(any())))] x: Missing }",
        "struct S(#[cfg(any())] Missing, #[cfg(all())] i32); fn f() { let S(x) = S(1); }",
        "#[repr(u8)] enum E { #[cfg(any())] V(Missing), A = 1, B }",
        "#[repr(u8)] enum E { #[cfg(any())] V(Missing), #[cfg(all())] A = 1 }",
        "fn f(#[cfg(any())] x: Missing, x: i32) {}",
        "struct S { #[cfg(any())] x: missing!(), value: i32 }",
        "enum E { #[cfg(any())] V { x: missing!() }, A }",
        "fn f(#[cfg_attr(all(), cfg(any()), allow(dead_code))] x: impl Missing, value: i32) {}",
        "unsafe extern \"C\" { fn f(#[cfg(any())] x: Missing, x: i32); }",
    ];
    let rejected = [
        "struct S { #[cfg(not())] x: i32 }",
        "struct S(#[cfg_attr(all(), cfg(not()))] i32);",
        "enum E { #[cfg(not())] V }",
        "fn f(#[cfg(not())] x: i32) {}",
        "type F = fn(#[cfg(not())] i32);",
        "fn f() { let c = |#[cfg_attr(all(), cfg(not()))] x: i32| x; }",
        "#[repr(u8)] enum E { #[cfg(any())] V }",
    ];
    let dir = std::env::temp_dir().join(format!("flux-cfg-members-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (sources, expected) in [(&accepted[..], true), (&rejected[..], false)] {
        for source in sources {
            let path = dir.join("probe.rs");
            std::fs::write(&path, source).unwrap();
            let output = std::process::Command::new("rustc")
                .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
                .arg(&path)
                .arg("-o")
                .arg(dir.join("probe.rmeta"))
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                expected,
                "{source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let (db, root, hir) = flux_hir::lower_text(&path, source);
            assert!(db.file(root).parse.errors().is_empty(), "{source}");
            assert_eq!(
                hir.diagnostics.is_empty(),
                expected,
                "{source}\n{}",
                hir.debug_dump()
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn inactive_negative_impl_is_pruned_before_support_checks() {
    let source = "struct S; #[cfg(any())] impl !Send for S {} fn main() {}";
    let (db, _, hir) = flux_hir::lower_text("negative.rs", source);
    assert!(db.syntax_errors().is_empty(), "{:?}", db.syntax_errors());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let (db, _, hir) = flux_hir::lower_text("negative.rs", "struct S; impl !Send for S {}");
    assert!(db.syntax_errors().is_empty());
    assert!(
        hir.diagnostics
            .iter()
            .any(|d| d.message == "negative impls are not supported yet")
    );
}
