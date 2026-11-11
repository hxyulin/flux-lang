//! Lint scopes compared with rustc, without masking unrelated errors.
use flux_hir::SourceDb;
use flux_intern::Symbol;
#[test]
fn overflow_levels_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-overflow-levels-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepted, warning) in [
        ("default", "pub const X:i8=255;", false, false),
        (
            "crate_allow",
            "#![allow(overflowing_literals)] pub const X:i8=255;",
            true,
            false,
        ),
        (
            "warn",
            "#[warn(overflowing_literals)] pub const X:i8=255;",
            true,
            true,
        ),
        (
            "local_deny",
            "#![allow(overflowing_literals)] #[deny(overflowing_literals)] pub const X:i8=255;",
            false,
            false,
        ),
        (
            "local_allow",
            "#![deny(overflowing_literals)] #[allow(overflowing_literals)] pub fn f()->i8{255}",
            true,
            false,
        ),
        (
            "forbid",
            "#![forbid(overflowing_literals)] #[allow(overflowing_literals)] pub const X:i8=0;",
            false,
            false,
        ),
        (
            "block",
            "pub fn f(){#[allow(overflowing_literals)] {let _x:i8=255;}}",
            true,
            false,
        ),
        (
            "statement",
            "pub fn f(){#[allow(overflowing_literals)] let _x:i8=255;}",
            true,
            false,
        ),
        (
            "pattern",
            "#[allow(overflowing_literals)] pub fn f(x:i8)->bool{matches!(x,255)}",
            true,
            false,
        ),
        (
            "conditional",
            "#[cfg_attr(all(),allow(overflowing_literals))] pub const X:i8=255;",
            true,
            false,
        ),
        (
            "inactive",
            "#[cfg(any())] #[expect(overflowing_literals)] pub const X:i8=0;",
            true,
            false,
        ),
        (
            "expect",
            "#[expect(overflowing_literals)] pub const X:i8=255;",
            true,
            false,
        ),
        (
            "expect_unfulfilled",
            "#[expect(overflowing_literals)] pub const X:i8=0;",
            true,
            true,
        ),
        (
            "expect_denied",
            "#![deny(unfulfilled_lint_expectations)] #[expect(overflowing_literals)] pub const X:i8=0;",
            false,
            false,
        ),
        (
            "expect_macro",
            "macro_rules! m{()=>{{#[expect(overflowing_literals)] let v:i8=255;v}}} pub fn f()->i8{m!()}",
            true,
            false,
        ),
        (
            "macro_allow",
            "macro_rules! m{()=>{{#[allow(overflowing_literals)] let v:i8=255;v}}} pub fn f()->i8{m!()}",
            true,
            false,
        ),
        (
            "caller_allow",
            "macro_rules! m{()=>{255i8}} #[allow(overflowing_literals)] pub fn f()->i8{m!()}",
            true,
            false,
        ),
        (
            "warnings_group",
            "#![allow(warnings)] #[warn(overflowing_literals)] pub const X:i8=255;",
            true,
            false,
        ),
        (
            "warnings_deny",
            "#![deny(warnings)] #[warn(overflowing_literals)] pub const X:i8=255;",
            false,
            false,
        ),
        (
            "repr_allow",
            "#![allow(overflowing_literals)] #[repr(i8)] pub enum E{A=128}",
            true,
            false,
        ),
        (
            "repr_warn",
            "#![warn(overflowing_literals)] #[repr(i8)] pub enum E{A=128}",
            true,
            true,
        ),
        (
            "generic_allow",
            "#![allow(overflowing_literals)] pub fn f<const N:i8>(){} pub fn g(){f::<255>();}",
            true,
            false,
        ),
        (
            "generic_warn",
            "#![warn(overflowing_literals)] pub fn f<const N:i8>(){} pub fn g(){f::<255>();}",
            true,
            true,
        ),
        (
            "hard_type_error",
            "#![allow(overflowing_literals)] pub const X:i8=\"bad\";",
            false,
            false,
        ),
        (
            "unsigned_neg",
            "#![allow(overflowing_literals)] pub const X:u8=-1;",
            false,
            false,
        ),
        ("malformed", "#[allow] pub const X:i8=0;", false, false),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "--out-dir",
            ])
            .arg(&dir)
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&file, source);
        let typed = flux_ty::check_file(&hir);
        assert_eq!(
            hir.diagnostics.is_empty() && typed.diagnostics.is_empty(),
            accepted,
            "Flux {name}: {:?} {:?}",
            hir.diagnostics,
            typed.diagnostics
        );
        if accepted {
            assert_eq!(
                !typed.warnings.is_empty(),
                warning,
                "{name}: {:?}",
                typed.warnings
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn external_module_attributes_are_inherited() {
    let dir = std::env::temp_dir().join(format!("flux-external-lints-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.rs"),
        "#[allow(overflowing_literals)] mod child;\n",
    )
    .unwrap();
    std::fs::write(dir.join("child.rs"), "pub const X:i8=255;").unwrap();
    let mut db = SourceDb::new();
    let root = db.load(&dir.join("main.rs")).unwrap();
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("probe"));
    let typed = flux_ty::check_file(&hir);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    std::fs::write(
        dir.join("main.rs"),
        "macro_rules! make{()=>{#[allow(overflowing_literals)] mod child;}}make!();\n",
    )
    .unwrap();
    let mut db = SourceDb::new();
    let root = db.load(&dir.join("main.rs")).unwrap();
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("probe"));
    let typed = flux_ty::check_file(&hir);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    std::fs::remove_dir_all(dir).unwrap();
}
