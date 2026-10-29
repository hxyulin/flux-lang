use flux_hir::{CrateId, SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_ty::{signatures, signatures_from};

#[test]
fn source_extension_preserves_checked_hidden_types_and_regions() {
    let a_text = r#"
        pub struct Borrowed<'a> { pub value: &'a u32 }
        pub trait Fetch { type Output; fn fetch(&self) -> Self::Output; }
        impl Fetch for u32 { type Output = u32; fn fetch(&self) -> u32 { *self } }
        pub fn hidden() -> impl Copy { 7u32 }
        pub fn borrow<'a>(value: &'a u32) -> Borrowed<'a> { Borrowed { value } }
    "#;
    let b_text = "pub fn forward<'a>(value: &'a u32) -> dep::Borrowed<'a> { dep::borrow(value) }";
    let mut db = SourceDb::new();
    let a = db.add("a.rs", a_text);
    let first = flux_hir::lower(&mut db, a, Symbol::intern("a"));
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    let checked = flux_ty::check_file(&first);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert!(!checked.sigs.hidden.is_empty());
    let preserved_hidden = checked.sigs.hidden.clone();
    let preserved_regions: Vec<_> = checked
        .sigs
        .regions
        .fns
        .iter()
        .map(|(id, value)| (id, value.clone()))
        .collect();
    let from = first.items.counts();
    let owner = CrateId(first.items.crates.len() as u32 - 1);
    let b = db.add("b.rs", b_text);
    let second = flux_hir::lower_source_on(
        &mut db,
        first.items,
        b,
        Symbol::intern("b"),
        vec![(Symbol::intern("dep"), owner)],
    );
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    let (extended, errors) = signatures_from(&second.items, checked.sigs, from, None);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(extended.hidden, preserved_hidden);
    for (id, value) in preserved_regions {
        assert_eq!(extended.regions.fns[id], value);
    }

    let mut baseline_db = SourceDb::new();
    let a = baseline_db.add("a.rs", a_text);
    let b = baseline_db.add("b.rs", b_text);
    let baseline = flux_hir::lower_sources(
        &mut baseline_db,
        &[
            SourceCrate {
                root: a,
                name: Symbol::intern("a"),
                dependencies: vec![],
            },
            SourceCrate {
                root: b,
                name: Symbol::intern("b"),
                dependencies: vec![(Symbol::intern("dep"), 0)],
            },
        ],
    );
    let (expected, errors) = signatures(&baseline.items);
    assert!(errors.is_empty(), "{errors:?}");
    for (id, sig) in expected.fns.iter() {
        assert_eq!(extended.fns[id], *sig);
    }
    for (id, regions) in expected.regions.fns.iter() {
        assert_eq!(extended.regions.fns[id], *regions);
    }
}

#[test]
fn loaded_type_provider_checks_identities_and_source_ownership() {
    let mut db = SourceDb::new();
    let root=db.add("dependency.rs","pub struct Wrapper<'a> { pub value: &'a u32 } pub const N: u32 = 3; pub fn value() -> impl Copy { N }");
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("dependency"));
    let typed = flux_ty::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let owner = CrateId(hir.items.crates.len() as u32 - 1);
    let indices = flux_hir::flib::CrateIndices::collect(&hir.items, owner);
    let provider =
        flux_ty::flib::LoadedTypes::new(&hir.items, typed.sigs.clone(), &[(owner, [7; 32])])
            .unwrap();
    assert_eq!(provider.hash(owner), Some([7; 32]));
    assert_eq!(provider.source_counts(), hir.items.counts());
    for &raw in &indices.functions {
        assert!(provider.owns_function(flux_hir::FunctionId::from_raw(raw)));
    }
    for &raw in &indices.consts {
        assert!(provider.owns_const(flux_hir::ConstId::from_raw(raw)));
    }
    assert!(
        flux_ty::flib::LoadedTypes::new(
            &hir.items,
            typed.sigs.clone(),
            &[(owner, [7; 32]), (owner, [8; 32])]
        )
        .is_err()
    );
    assert!(
        flux_ty::flib::LoadedTypes::new(
            &hir.items,
            flux_ty::Signatures::default(),
            &[(owner, [7; 32])]
        )
        .is_err()
    );
    let mut missing_hidden = typed.sigs;
    missing_hidden.hidden.clear();
    assert!(
        flux_ty::flib::LoadedTypes::new(&hir.items, missing_hidden, &[(owner, [7; 32])]).is_err()
    );
    let source = db.add("consumer.rs", "pub fn forward() { let _ = dep::value(); }");
    let consumer = flux_hir::lower_source_on(
        &mut db,
        hir.items,
        source,
        Symbol::intern("consumer"),
        vec![(Symbol::intern("dep"), owner)],
    );
    assert!(
        consumer.diagnostics.is_empty(),
        "{:?}",
        consumer.diagnostics
    );
    let (sigs, errors) = provider.extend_source_signatures(&consumer.items, None);
    assert!(errors.is_empty(), "{errors:?}");
    for (id, _) in consumer.bodies.iter() {
        assert!(!provider.owns_function(id));
        assert!(sigs.fns.get(id).is_some());
    }
}
