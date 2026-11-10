use flux_cli::flib::semantic::{CheckedCrate, Dependency, load_checked, write_checked};
use flux_hir::flib::CrateIndices;

fn empty_mir() -> flux_mir::FileMir {
    flux_mir::FileMir {
        panic_strategy: flux_mir::PanicStrategy::Abort,
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
fn checked_crate_loads_without_source_or_dependency_checking() {
    let (_db, _, hir) = flux_hir::lower_text(
        "a.rs",
        "pub struct Packet<T> { pub value: T } pub fn id<T>(x: T) -> T { x } pub const N: usize = 3; pub const fn add(x: u32) -> u32 { x + 1 } #[macro_export] macro_rules! number { () => { $crate::N }; }",
    );
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let own = hir.items.main_crate();
    let deps: Vec<_> = hir
        .items
        .crates
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != own.0 as usize)
        .map(|(i, c)| Dependency {
            krate: flux_hir::CrateId(i as u32),
            hash: flux_hir::flib::digest(c.name.as_str().as_bytes()),
            indices: CrateIndices::collect(&hir.items, flux_hir::CrateId(i as u32)),
        })
        .collect();
    let bytes = write_checked(CheckedCrate {
        items: &hir.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: own,
        hash: [7; 32],
        dependencies: &deps,
        object: None,
        required_instances: &[],
        freshness: None,
    })
    .unwrap();
    let (mut db, _, base) = flux_hir::lower_text("unrelated.rs", "fn ignored() {}");
    let mut sigs = flux_ty::signatures(&base.items).0;
    let mut items = base.items;
    let mut loaded_mir = empty_mir();
    let loaded = load_checked(
        &bytes,
        &mut db,
        &mut items,
        &mut sigs,
        &mut loaded_mir,
        &deps,
    )
    .unwrap();
    assert_eq!(loaded.dependency.hash, [7; 32]);
    let krate = loaded.dependency.krate;
    assert_ne!(krate, own);
    let id = items
        .functions
        .iter()
        .find(|(_, f)| items.modules[f.loc.module].krate == krate && f.name.as_str() == "id")
        .unwrap()
        .0;
    assert!(loaded_mir.bodies.get(id).is_some());
    assert_eq!(sigs[id].params.len(), 1);
    for index in loaded.dependency.indices.files {
        let file = db.file(flux_hir::FileId(index));
        assert!(
            file.parse.tree().text().is_empty(),
            "dependency source was parsed"
        );
        assert!(
            !file.text().is_empty(),
            "dependency diagnostics lost their source"
        );
    }
    let again = write_checked(CheckedCrate {
        items: &hir.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: own,
        hash: [7; 32],
        dependencies: &deps,
        object: None,
        required_instances: &[],
        freshness: None,
    })
    .unwrap();
    assert_eq!(bytes, again);
    let linkable = write_checked(CheckedCrate {
        items: &hir.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: own,
        hash: [7; 32],
        dependencies: &deps,
        object: Some(vec![1, 2, 3]),
        required_instances: &[],
        freshness: None,
    })
    .unwrap();
    assert!(
        flux_cli::flib::semantic::manifest(&linkable)
            .unwrap()
            .linkable
    );
    let mut archive = flux_cli::flib::Archive::decode(&linkable).unwrap();
    archive.object.as_mut().unwrap()[0] ^= 1;
    let damaged = archive.encode().unwrap();
    let error = flux_cli::flib::semantic::manifest(&damaged).unwrap_err();
    assert!(error.contains("object checksum"), "{error}");
    let mut metadata = flux_cli::flib::Metadata::decode(
        &archive.metadata,
        flux_cli::flib::compiler_identity().unwrap(),
    )
    .unwrap();
    let mut e = flux_hir::flib::Encoder::new();
    flux_hir::flib::Encode::encode(
        &(
            2u32,
            "wrong-target".to_owned(),
            None::<Vec<u8>>,
            Vec::<u8>::new(),
        ),
        &mut e,
    )
    .unwrap();
    metadata
        .sections
        .iter_mut()
        .find(|s| s.tag == 9)
        .unwrap()
        .bytes = e.into_bytes();
    archive.metadata = metadata.encode().unwrap();
    archive.object = None;
    let error = flux_cli::flib::semantic::manifest(&archive.encode().unwrap()).unwrap_err();
    assert!(error.contains("target mismatch"), "{error}");
}

#[test]
fn call_positions_inside_utf8_codepoints_are_rejected() {
    let source = "// é\npub fn value() -> u32 { 42 }";
    let (_db, _, hir) = flux_hir::lower_text("utf8.rs", source);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mut mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let own = hir.items.main_crate();
    let function = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| hir.items.modules[f.loc.module].krate == own)
        .unwrap()
        .0;
    mir.bodies.get_mut(function).unwrap().fn_spans.insert(
        flux_mir::Body::entry(),
        source.find('é').unwrap() as u32 + 1,
    );
    let deps: Vec<_> = hir
        .items
        .crates
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != own.0 as usize)
        .map(|(i, c)| {
            let krate = flux_hir::CrateId(i as u32);
            Dependency {
                krate,
                hash: flux_hir::flib::digest(c.name.as_str().as_bytes()),
                indices: CrateIndices::collect(&hir.items, krate),
            }
        })
        .collect();
    let bytes = write_checked(CheckedCrate {
        items: &hir.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: own,
        hash: [9; 32],
        dependencies: &deps,
        object: None,
        required_instances: &[],
        freshness: None,
    })
    .unwrap();
    let (mut db, _, base) = flux_hir::lower_text("consumer.rs", "");
    let mut sigs = flux_ty::signatures(&base.items).0;
    let error = load_checked(
        &bytes,
        &mut db,
        &mut base.items.clone(),
        &mut sigs,
        &mut empty_mir(),
        &deps,
    )
    .unwrap_err();
    assert!(error.contains("invalid MIR source range"), "{error}");
}

#[test]
fn obsolete_linkage_abi_is_rejected_before_reading_instance_requirements() {
    use flux_cli::flib::{Archive, Metadata, Section};
    use flux_hir::flib::{Encode, Encoder};
    let mut identity = Encoder::new();
    ("old-library", vec![3u8; 32], Vec::<Vec<u8>>::new())
        .encode(&mut identity)
        .unwrap();
    let mut linkage = Encoder::new();
    // This is the actual ABI 1 layout: no structural-instance payload follows.
    (
        1u32,
        flux_codegen::Target::host().unwrap().triple(),
        None::<Vec<u8>>,
    )
        .encode(&mut linkage)
        .unwrap();
    let identity = identity.into_bytes();
    let linkage = linkage.into_bytes();
    let metadata = Metadata {
        compiler: flux_cli::flib::compiler_identity().unwrap(),
        sections: (1..=9)
            .map(|tag| Section {
                tag,
                bytes: match tag {
                    1 => identity.clone(),
                    9 => linkage.clone(),
                    _ => Vec::new(),
                },
            })
            .collect(),
    }
    .encode()
    .unwrap();
    let bytes = Archive {
        metadata,
        object: None,
    }
    .encode()
    .unwrap();
    let error = flux_cli::flib::semantic::manifest(&bytes).unwrap_err();
    assert!(error.contains("linkage ABI 1"), "{error}");
    assert!(error.contains("rebuild the artifact"), "{error}");
}
