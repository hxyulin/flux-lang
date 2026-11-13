#[test]
fn discriminant_captures_textual_macro_scope() {
    let source = include_str!("../../../tests/run/enum_discriminant_macros.rs");
    let (_, _, hir) = flux_hir::lower_text("enum-macros.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
}
#[test]
fn later_textual_macro_is_not_visible_to_discriminant() {
    let source = "#[repr(u32)]enum E{V=later!()}macro_rules! later{()=>{1}}fn main(){}";
    let root = std::env::temp_dir().join(format!("flux-enum-macro-scope-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("main.rs");
    std::fs::write(&path, source).unwrap();
    let output = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("out.rmeta"))
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "Rustc unexpectedly accepted forward macro use"
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(
        hir.diagnostics
            .iter()
            .any(|d| d.message.contains("cannot find macro")),
        "{:?}",
        hir.diagnostics
    );
    std::fs::remove_dir_all(root).unwrap();
}
