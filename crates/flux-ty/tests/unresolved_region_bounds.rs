//! Invalid bounds must diagnose without indexing compacted region tables.

#[test]
fn unresolved_bounds_with_associated_bindings_do_not_panic() {
    for source in [
        "trait Known { type Item; } trait Test { type Output: Missing + Known<Item = ()>; }",
        "trait Known { type Item; } fn test<T>() where T: Missing, T: Known<Item = ()>, T: Known<Item = ()> {}",
        "trait Known { type Item; } trait Test<T> where T: Missing, T: Known<Item = ()> {}",
    ] {
        let (_, _, hir) = flux_hir::lower_text_bare("invalid.rs", source);
        let typed = flux_ty::check_file(&hir);
        assert!(
            !typed.diagnostics.is_empty(),
            "expected errors for {source}"
        );
    }
}
