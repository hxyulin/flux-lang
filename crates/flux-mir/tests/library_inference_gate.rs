//! The source library cache must not invent work omitted by source inference.

#[test]
fn cached_library_body_requires_the_compilations_inference_record() {
    let (_db, _, hir) =
        flux_hir::lower_text("library_gate.rs", "fn main() { println!(\"{}\", 1u32); }");
    let mut typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let print = typed
        .bodies
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            id.into_raw() < hir.items.core.functions
                && hir.items.functions[id].name.as_str() == "_print"
        })
        .expect("the source print call reaches the cached library body");
    let complete = flux_mir::build_file(&hir, &typed);
    assert!(
        complete.diagnostics.is_empty(),
        "{:?}",
        complete.diagnostics
    );
    assert!(
        complete.bodies.get(print).is_some(),
        "the complete inference traversal builds its callee"
    );

    // Source inference owns this decision even though LibraryMir has a checked
    // copy. Imported user MIR deliberately has no inference record and follows
    // the separate provider branch, covered by the loaded-state regressions.
    assert!(typed.bodies.remove(print).is_some());
    let partial = flux_mir::build_file(&hir, &typed);
    assert!(partial.bugs.is_empty(), "{:?}", partial.bugs);
    assert!(
        partial.bodies.get(print).is_none(),
        "the cache must preserve the source inference gate"
    );
}
