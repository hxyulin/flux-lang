//! Local glob cycles cannot resolve a completed dependency in a bare graph.

#[test]
fn standalone_glob_cycles_do_not_branch_through_import_permutations() {
    let mut source = String::from("use missing::Unknown;\n");
    for index in 0..12 {
        source.push_str(&format!("use unknown_{index}::*;\n"));
    }
    source.push_str("fn test(value: Unknown) {}\n");
    let (_, _, hir) = flux_hir::lower_text_bare("glob_cycles.rs", &source);
    assert_eq!(hir.items.functions.len(), 1);
    let (_, _, with_builtins) = flux_hir::lower_text("glob_cycles.rs", &source);
    assert!(!with_builtins.diagnostics.is_empty());
}
