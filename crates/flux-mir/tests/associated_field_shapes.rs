//! Nongeneric associated field aliases use their represented storage types.
#[test]
fn associated_fields_are_normalized_without_type_arguments() {
    let (_, _, hir) = flux_hir::lower_text(
        "associated-fields.rs",
        include_str!("../../../tests/run/associated_field_shapes.rs"),
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    for name in ["Record", "TwoWords", "Storage", "Borrowed", "Choice"] {
        let (id, data) = hir
            .items
            .structs
            .iter()
            .find(|(_, item)| item.name.as_str() == name)
            .unwrap();
        let ty = flux_ty::Ty::adt(id, data.name, &[]);
        let fields = flux_solve::contents(&typed.sigs, ty).unwrap();
        assert!(
            fields.iter().all(|field| !field.has_aliases()),
            "{name}: {fields:?}"
        );
        assert!(
            fields
                .iter()
                .any(|field| *field == flux_ty::Ty::int(flux_ty::IntTy::U64)),
            "{name}: {fields:?}"
        );
    }
}
