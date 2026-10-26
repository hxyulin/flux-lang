//! Aggregate copy elimination preserves by-value inputs, output aliases and Drop.
mod support;

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn aggregate_storage_preserves_values_and_ownership() {
    support::oracle::compare_fixture("aggregate_storage");
}

#[test]
fn returned_arrays_do_not_allocate_redundant_local_buffers() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/run/aggregate_storage.rs");
    let mut db = flux_hir::SourceDb::new();
    let file = db.load(&path).unwrap();
    let hir = flux_hir::lower_sources(
        &mut db,
        &[flux_hir::SourceCrate {
            root: file,
            name: flux_intern::Symbol::intern("storage"),
            dependencies: Vec::new(),
        }],
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for name in ["make", "through"] {
        let functions: Vec<_> = module
            .functions
            .iter()
            .filter(|f| f.name.contains(name))
            .collect();
        assert_eq!(functions.len(), 1, "ambiguous {name}");
        let bytes: u64 = functions[0]
            .stack_slots()
            .map(|(_, data)| u64::from(data.size))
            .sum();
        assert_eq!(
            bytes, 0,
            "{name} allocated local buffers for a 32 KiB return: {bytes} bytes"
        );
    }
}
