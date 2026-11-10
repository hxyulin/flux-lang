//! Imported constant functions participate in array lengths without inference.

use flux_hir::{CrateId, SourceDb};
use flux_intern::Symbol;
use flux_mir::imported::ImportedMir;
use flux_ty::flib::LoadedTypes;

#[test]
fn source_array_lengths_use_imported_mir_and_values_without_inference() {
    let mut db = SourceDb::new();
    let a = db.add(
        "a.rs",
        r#"
        pub const fn add(x:usize)->usize { x + 1 }
        pub const BASE:usize=add(4);
        pub fn identity<T>(x:T)->T { x }
        pub fn hidden()->impl Copy { 3u32 }
    "#,
    );
    let first = flux_hir::lower(&mut db, a, Symbol::intern("a"));
    let typed = flux_mir::check_file(&first);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&first, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let owner = first.items.main_crate();
    let counts = first.items.counts();
    let loaded = LoadedTypes::new(&first.items, typed.sigs, &[(owner, [7; 32])]).unwrap();
    let mut own = flux_mir::FileMir {
        panic_strategy: flux_mir::PanicStrategy::Abort,
        bodies: Default::default(),
        closures: Default::default(),
        consts: Default::default(),
        const_values: Default::default(),
        discriminants: Default::default(),
        diagnostics: Vec::new(),
        bugs: Vec::new(),
    };
    for (id, body) in mir.bodies.iter() {
        if loaded.owns_function(id) {
            own.bodies.insert(id, body.clone());
        }
    }
    for (id, body) in mir.consts.iter() {
        if loaded.owns_const(id) {
            own.consts.insert(id, body.clone());
        }
    }
    for (id, value) in mir.const_values.items.iter() {
        if loaded.owns_const(id) {
            own.const_values.items.insert(id, value.clone());
        }
    }
    let imported = ImportedMir::new(own, counts).unwrap();
    let roots = imported.required_functions(&first.items, loaded.signatures());
    let b = db.add(
        "b.rs",
        r#"
        pub const SIZE:usize=dep::add(dep::BASE);
        pub const SECOND:usize=SIZE + 1;
        pub const TABLE:[u8; SECOND]=[0; SECOND];
        pub fn use_table(_: [u8; TABLE.len()])->usize { TABLE.len() }
        pub fn use_array(_: [u8; SIZE])->usize { SIZE }
        pub fn forward()->u32 { dep::identity(3u32) }
    "#,
    );
    let consumer = flux_hir::lower_source_on(
        &mut db,
        first.items,
        b,
        Symbol::intern("b"),
        vec![(Symbol::intern("dep"), owner)],
    );
    assert!(
        consumer.diagnostics.is_empty(),
        "{:?}",
        consumer.diagnostics
    );
    let checked = flux_mir::check_file_from(&consumer, &loaded, &imported, &roots.functions);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert!(
        checked
            .bodies
            .iter()
            .all(|(id, _)| !loaded.owns_function(id)),
        "imported functions must have no inference result"
    );
    assert!(
        checked.consts.iter().all(|(id, _)| !loaded.owns_const(id)),
        "imported constants must have no inference result"
    );
    let current = CrateId(consumer.items.crates.len() as u32 - 1);
    let array = consumer
        .items
        .functions
        .iter()
        .find(|(_, f)| {
            consumer.items.modules[f.loc.module].krate == current && f.name.as_str() == "use_array"
        })
        .unwrap()
        .0;
    assert_eq!(checked.sigs[array].params[0].array_len(), Some(6));
    let table = consumer
        .items
        .functions
        .iter()
        .find(|(_, f)| {
            consumer.items.modules[f.loc.module].krate == current && f.name.as_str() == "use_table"
        })
        .unwrap()
        .0;
    assert_eq!(checked.sigs[table].params[0].array_len(), Some(7));
    for (id, signature) in loaded.signatures().fns.iter() {
        assert!(
            std::ptr::eq(signature, &checked.sigs.fns[id]),
            "imported signature {id:?} was copied"
        );
    }
    let mir = flux_mir::build_file_from_owned(&consumer, &checked, imported, &roots.functions);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    assert!(mir.bodies.get(array).is_some());

    assert_eq!(
        checked.sigs.hidden,
        loaded.signatures().hidden,
        "dependency opaque types must survive source extension"
    );
}
