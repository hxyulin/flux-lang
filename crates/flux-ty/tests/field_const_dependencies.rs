//! Storage dependency follows fields rather than every generic argument.
#[test]
fn stored_layout_cuts_indirection_but_not_owned_wrappers() {
    let (_, _, hir) = flux_hir::lower_text(
        "layout.rs",
        r#"
        struct S { byte: u8 }
        struct Pointer<T>(*const T);
        struct Owned<T>(T);
        fn types(
            a: Option<Box<S>>, b: Option<Vec<S>>, c: core::marker::PhantomData<S>,
            d: Option<&S>, e: Option<*const S>, f: Option<fn(S)>,
            g: Pointer<Pointer<S>>, h: Owned<Owned<&S>>,
            i: S, j: Option<S>, k: core::mem::ManuallyDrop<S>,
            l: core::mem::MaybeUninit<S>, m: [S;0], n: Owned<Owned<S>>,
        ) {}
    "#,
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let (sigs, errors) = flux_ty::signatures(&hir.items);
    assert!(errors.is_empty(), "{errors:?}");
    let owner = hir
        .items
        .structs
        .iter()
        .find(|(_, s)| s.name.as_str() == "S")
        .unwrap()
        .0;
    let function = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "types")
        .unwrap()
        .0;
    for (&ty, expected) in sigs[function].params.iter().zip([
        false, false, false, false, false, false, false, false, true, true, true, true, true, true,
    ]) {
        assert_eq!(
            flux_ty::const_dependencies::stores_adt(&hir.items, &sigs, ty, owner),
            Ok(expected),
            "{ty}"
        );
    }
}
