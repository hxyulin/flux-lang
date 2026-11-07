//! Private instance names remain unique and repeatable within a module.

#[test]
fn repeated_lowering_preserves_private_instance_names_and_references() {
    let source = include_str!("../../../tests/run/private_instance_names.rs");
    let (_, _, hir) = flux_hir::lower_text("main.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let first = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let second = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    assert_eq!(first.to_string(), second.to_string());
    let mut names = std::collections::HashSet::new();
    for function in &first.functions {
        assert!(names.insert(&function.name), "duplicate {}", function.name);
    }
    keel_ir::verify_module(&first).unwrap();
}
