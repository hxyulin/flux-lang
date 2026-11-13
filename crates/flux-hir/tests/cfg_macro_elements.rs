//! A macro expression inherits elision permission only from its list element.
#[test]
fn required_macro_expression_cannot_disappear() {
    for (name, source) in [
        (
            "initializer",
            "macro_rules! gone{()=>{#[cfg(any())]{missing}}}fn main(){let x=gone!();}",
        ),
        (
            "operand",
            "macro_rules! gone{()=>{#[cfg(any())]{missing}}}fn main(){let x=1+gone!();}",
        ),
        (
            "nested_operand",
            "macro_rules! gone{()=>{#[cfg(any())]{missing}}}macro_rules! outer{()=>{#[cfg(all())]{1+gone!()}}}fn main(){let x=[outer!()];}",
        ),
        (
            "malformed",
            "macro_rules! bad{()=>{#[cfg(not())]{1}}}fn main(){let x=[bad!()];}",
        ),
    ] {
        let root =
            std::env::temp_dir().join(format!("flux-cfg-expansion-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("main.rs");
        std::fs::write(&path, source).unwrap();
        let result = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("out.rmeta"))
            .output()
            .unwrap();
        assert!(
            !result.status.success(),
            "Rustc unexpectedly accepted {name}"
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            !hir.diagnostics.is_empty(),
            "Flux unexpectedly accepted {name}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn nested_macro_list_elements_are_pruned_before_resolution() {
    let source = include_str!("../../../tests/run/cfg_macro_elements.rs");
    let (_, _, hir) = flux_hir::lower_text("cfg-elements.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
}
