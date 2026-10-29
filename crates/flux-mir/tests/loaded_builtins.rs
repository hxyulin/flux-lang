//! Trusted library metadata replaces source inference and copies only reached MIR.

use std::sync::Arc;

use flux_hir::flib::{Decoder, Encoder, read_builtin_suite, write_builtin_suite};
use flux_hir::{Core, CrateId, SourceDb};
use flux_intern::Symbol;
use flux_mir::imported::ImportedMir;
use flux_ty::Signatures;
use flux_ty::flib::LoadedTypes;

fn empty_mir() -> flux_mir::FileMir {
    flux_mir::FileMir {
        bodies: Default::default(),
        closures: Default::default(),
        consts: Default::default(),
        const_values: Default::default(),
        discriminants: Default::default(),
        diagnostics: Vec::new(),
        bugs: Vec::new(),
    }
}

#[test]
fn decoded_builtin_prefix_matches_source_without_dependency_inference() {
    let core = Core::get().hir();
    let typed = flux_ty::check_file_all(core);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let functions: Vec<_> = core
        .items
        .functions
        .iter()
        .filter(|(_, f)| f.has_body)
        .map(|(id, _)| id)
        .collect();
    let mir = flux_mir::build_file_with_roots(core, &typed, &functions);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    assert!(mir.bugs.is_empty(), "{:?}", mir.bugs);
    let mut encoder = Encoder::new();
    write_builtin_suite(&core.items, &SourceDb::new(), &mut encoder).unwrap();
    typed.sigs.encode_flib(&mut encoder).unwrap();
    mir.encode_flib(&mut encoder).unwrap();
    let bytes = encoder.into_bytes();
    let mut decoder = Decoder::new(&bytes);
    let (mut db, items) = read_builtin_suite(&mut decoder).unwrap();
    let mut signatures = Signatures::default();
    signatures.decode_flib_into(&mut decoder, &items).unwrap();
    let mut mir = empty_mir();
    mir.decode_flib_into(&mut decoder, &items, &signatures)
        .unwrap();
    decoder.finish().unwrap();
    assert!(items.has_artifact_builtins());
    assert!(
        db.files()
            .all(|(_, file)| file.parse.tree().text().is_empty())
    );
    let hashes: Vec<_> = (0..items.crates.len())
        .map(|index| (CrateId(index as u32), [index as u8 + 1; 32]))
        .collect();
    assert!(
        LoadedTypes::new(&items, signatures.clone(), &hashes).is_err(),
        "ordinary imports must reject privileged builtins"
    );
    assert!(
        LoadedTypes::with_builtin_prefix(&items, signatures.clone(), &[]).is_err(),
        "every builtin crate needs an authenticated identity"
    );
    assert!(
        LoadedTypes::with_builtin_prefix(&core.items, typed.sigs.clone(), &hashes).is_err(),
        "source items cannot claim the metadata provider marker"
    );
    assert!(
        ImportedMir::with_builtin_prefix(&items, empty_mir(), Arc::new(empty_mir())).is_err(),
        "an incomplete builtin snapshot must be rejected"
    );
    let loaded = LoadedTypes::with_builtin_prefix(&items, signatures, &hashes).unwrap();
    let suite = Arc::new(mir);
    let imported =
        ImportedMir::with_builtin_prefix(&items, empty_mir(), Arc::clone(&suite)).unwrap();
    assert_eq!(Arc::strong_count(&suite), 2);
    assert_eq!(
        imported.mir().bodies.iter().count(),
        0,
        "suite functions must stay shared"
    );
    let mut seeded = empty_mir();
    imported.seed_into(&mut seeded).unwrap();
    assert_eq!(
        seeded.bodies.iter().count(),
        0,
        "seeding must not materialize shared suite functions"
    );
    let roots = imported.required_functions(&items, loaded.signatures());
    assert!(
        roots.functions.len() < functions.len() / 2,
        "unused suite functions must not be roots"
    );
    let source = r#"
        const N: usize = 3u32.count_ones() as usize + 1;
        const ID: std::any::TypeId = std::any::TypeId::of::<u32>();
        fn main() {
            let values: [u8; N] = [1,2,3];
            let mapped: Vec<u32> = values.iter().map(|value| *value as u32 + 1).collect();
            let value: &dyn std::any::Any = &42u32;
            assert_eq!(value.type_id(), ID);
            assert_eq!(mapped.len(), 3);
        }
    "#;
    let root = db.add("main.rs", source);
    let hir = flux_hir::lower_source_on(&mut db, items, root, Symbol::intern("main"), Vec::new());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    assert!(flux_ty::LibraryTypes::of(&hir).is_none());
    assert!(
        hir.body(functions[0]).is_none(),
        "loaded builtins have no lowered source bodies"
    );
    let checked = flux_mir::check_file_from(&hir, &loaded, &imported, &roots.functions);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert!(
        checked
            .bodies
            .iter()
            .all(|(id, _)| id.into_raw() >= hir.items.core.functions)
    );
    assert!(
        checked
            .consts
            .iter()
            .all(|(id, _)| id.into_raw() >= hir.items.core.consts)
    );
    let result = flux_mir::build_file_from(&hir, &checked, &imported, &roots.functions);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.bugs.is_empty(), "{:?}", result.bugs);
    assert!(
        result.bodies.iter().count() < functions.len() / 2,
        "unused builtin templates must remain unmaterialized"
    );
    let (_, _, normal) = flux_hir::lower_text("main.rs", source);
    let normal_typed = flux_mir::check_file(&normal);
    assert!(
        normal_typed.diagnostics.is_empty(),
        "{:?}",
        normal_typed.diagnostics
    );
    let normal_mir = flux_mir::build_file(&normal, &normal_typed);
    assert!(
        normal_mir.diagnostics.is_empty(),
        "{:?}",
        normal_mir.diagnostics
    );
    assert_eq!(checked.debug_dump(&hir), normal_typed.debug_dump(&normal));
    assert_eq!(result.debug_dump(&hir), normal_mir.debug_dump(&normal));
}
