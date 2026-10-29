//! Metadata round trips of existing native fixtures through an offset crate graph.

use std::path::Path;

use flux_cli::flib::semantic::{CheckedCrate, Dependency, load_checked, write_checked};
use flux_hir::flib::CrateIndices;
use flux_intern::Symbol;

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

fn round_trip(fixture: &str) {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = repository.join("tests/run").join(fixture);
    let mut source_db = flux_hir::SourceDb::new();
    let root = source_db.load(&path).unwrap();
    let hir = flux_hir::lower(&mut source_db, root, Symbol::intern("corpus"));
    assert!(
        hir.diagnostics.is_empty(),
        "{fixture}: HIR {:?}",
        hir.diagnostics
    );
    let typed = flux_mir::check_file(&hir);
    assert!(
        typed.diagnostics.is_empty(),
        "{fixture}: types {:?}",
        typed.diagnostics
    );
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(
        mir.diagnostics.is_empty(),
        "{fixture}: MIR {:?}",
        mir.diagnostics
    );
    let own = hir.items.main_crate();
    let dependencies: Vec<_> = hir
        .items
        .crates
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != own.0 as usize)
        .map(|(index, data)| {
            let krate = flux_hir::CrateId(index as u32);
            Dependency {
                krate,
                hash: flux_hir::flib::digest(data.name.as_str().as_bytes()),
                indices: CrateIndices::collect(&hir.items, krate),
            }
        })
        .collect();
    let hash = flux_hir::flib::digest(fixture.as_bytes());
    let archive = write_checked(CheckedCrate {
        items: &hir.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: own,
        hash,
        dependencies: &dependencies,
        object: None,
        freshness: None,
    })
    .unwrap_or_else(|error| panic!("{fixture}: encode: {error}"));

    let (mut target_db, _, mut target_hir) = flux_hir::lower_text(
        "flib-offset.rs",
        r#"
        pub struct Offset(u32);
        pub trait OffsetTrait { fn offset(&self) -> u32; }
        impl OffsetTrait for Offset { fn offset(&self) -> u32 { self.0 } }
        pub const OFFSET: usize = 5;
        fn ignored() -> usize { OFFSET }
    "#,
    );
    let target_typed = flux_mir::check_file(&target_hir);
    assert!(
        target_typed.diagnostics.is_empty(),
        "offset types: {:?}",
        target_typed.diagnostics
    );
    let mut target_sigs = target_typed.sigs;
    let mut target_mir = empty_mir();
    let loaded = load_checked(
        &archive,
        &mut target_db,
        &mut target_hir.items,
        &mut target_sigs,
        &mut target_mir,
        &dependencies,
    )
    .unwrap_or_else(|error| panic!("{fixture}: decode: {error}"));
    assert_ne!(
        loaded.dependency.krate, own,
        "{fixture}: crate IDs must shift"
    );
    let original_count = mir
        .bodies
        .iter()
        .filter(|(id, _)| hir.items.modules[hir.items.functions[*id].loc.module].krate == own)
        .count();
    assert_eq!(
        target_mir.bodies.iter().count(),
        original_count,
        "{fixture}: imported function bodies"
    );
    for file in &loaded.dependency.indices.files {
        assert!(
            target_db
                .file(flux_hir::FileId(*file))
                .parse
                .tree()
                .text()
                .is_empty(),
            "{fixture}: dependency source was parsed"
        );
    }
    let errors = flux_mir::validate::validate_file(&target_hir, &target_sigs, &target_mir);
    assert!(
        errors.is_empty(),
        "{fixture}: loaded MIR: {}",
        errors.join("\n")
    );
    let repeated = write_checked(CheckedCrate {
        items: &target_hir.items,
        sigs: &target_sigs,
        mir: &target_mir,
        krate: loaded.dependency.krate,
        hash,
        dependencies: &dependencies,
        object: None,
        freshness: None,
    })
    .unwrap_or_else(|error| panic!("{fixture}: re-encode: {error}"));
    assert!(
        archive == repeated,
        "{fixture}: canonical artifact bytes changed after remapping at byte {:?}, lengths {} and {}",
        archive.iter().zip(&repeated).position(|(a, b)| a != b),
        archive.len(),
        repeated.len()
    );
}

macro_rules! fixture {
    ($test:ident, $file:literal) => {
        #[test]
        fn $test() {
            round_trip($file);
        }
    };
}
fixture!(generic_functions, "generics.rs");
fixture!(generic_structs, "generic_structs.rs");
fixture!(closures, "closures.rs");
fixture!(closure_captures, "closure_captures.rs");
fixture!(constants, "consts.rs");
fixture!(const_generics, "const_generics.rs");
fixture!(enums, "enums.rs");
fixture!(trait_objects, "trait_objects.rs");
fixture!(opaque_regions, "opaque_bound_regions.rs");
fixture!(function_pointer_regions, "fn_pointer_regions.rs");
fixture!(runtime_type_identity, "any_type_id.rs");
fixture!(constant_trait_objects, "const_trait_objects.rs");
