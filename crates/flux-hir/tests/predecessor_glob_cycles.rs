//! Glob resolution during and after item collection.

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

#[test]
fn earlier_globs_see_bindings_added_to_their_sources_on_later_passes() {
    let (_, _, hir) = flux_hir::lower_text_bare(
        "glob_chain.rs",
        r#"
        mod first { pub use crate::second::*; }
        mod second { pub use crate::third::*; }
        mod third { pub use crate::leaf::*; }
        mod leaf { pub fn value() {} }
        fn test() { first::value(); }
        "#,
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
}
