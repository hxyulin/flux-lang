use flux_cli::flib::semantic::{Dependency, crate_hash};
use flux_hir::flib::CrateIndices;
use flux_intern::Symbol;
use std::collections::BTreeMap;
use std::path::Path;

#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use scratch::Scratch;

fn scratch() -> Scratch {
    Scratch::new(std::env::temp_dir().join("flux-flib-tests"))
}

fn hash_inputs(path: &Path, value: &str, configuration: &[u8]) -> [u8; 32] {
    let mut db = flux_hir::SourceDb::new();
    let root = db.load(path).unwrap();
    db.set_crate_env(
        root,
        BTreeMap::from([("VALUE".to_owned(), value.to_owned())]),
    )
    .unwrap();
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("inputs"));
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    crate_hash(&hir.items, hir.items.main_crate(), &[], configuration).unwrap()
}

#[test]
fn crate_identity_includes_observed_file_and_environment_contents() {
    let scratch = scratch();
    let source = scratch.path().join("lib.rs");
    let data = scratch.path().join("data.txt");
    std::fs::write(&source,"pub const TEXT: &str = include_str!(\"data.txt\"); pub const BYTES: &[u8] = include_bytes!(\"data.txt\"); pub const ENV: Option<&str> = option_env!(\"VALUE\");").unwrap();
    std::fs::write(&data, "alpha").unwrap();
    let original = hash_inputs(&source, "first", b"debug");
    assert_eq!(original, hash_inputs(&source, "first", b"debug"));
    assert_ne!(original, hash_inputs(&source, "second", b"debug"));
    assert_ne!(original, hash_inputs(&source, "first", b"release"));
    std::fs::write(&data, "bravo").unwrap();
    assert_ne!(original, hash_inputs(&source, "first", b"debug"));
    std::fs::write(&data, "alpha").unwrap();
    assert_eq!(original, hash_inputs(&source, "first", b"debug"));
}

fn graph_hash(padding: bool, alias: &str, dependency_hash: [u8; 32]) -> [u8; 32] {
    let mut db = flux_hir::SourceDb::new();
    let mut graph = Vec::new();
    if padding {
        let root = db.add("padding.rs", "pub struct Unrelated; fn unused() {}");
        graph.push(flux_hir::SourceCrate {
            root,
            name: Symbol::intern("padding"),
            dependencies: Vec::new(),
        });
    }
    let a_index = graph.len();
    let a = db.add("dependency.rs", "pub fn value()->u32{7}");
    graph.push(flux_hir::SourceCrate {
        root: a,
        name: Symbol::intern("dependency"),
        dependencies: Vec::new(),
    });
    let b = db.add("consumer.rs", "pub fn value()->u32{8}");
    graph.push(flux_hir::SourceCrate {
        root: b,
        name: Symbol::intern("consumer"),
        dependencies: vec![(Symbol::intern(alias), a_index)],
    });
    let hir = flux_hir::lower_sources(&mut db, &graph);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let own = hir.items.main_crate();
    let a = flux_hir::CrateId(own.0 - 1);
    let dependencies = [Dependency {
        krate: a,
        hash: dependency_hash,
        indices: CrateIndices::collect(&hir.items, a),
    }];
    crate_hash(&hir.items, own, &dependencies, b"config").unwrap()
}

#[test]
fn crate_identity_uses_dependency_aliases_and_hashes_instead_of_arena_offsets() {
    let original = graph_hash(false, "dep", [1; 32]);
    assert_eq!(original, graph_hash(true, "dep", [1; 32]));
    assert_ne!(original, graph_hash(false, "renamed", [1; 32]));
    assert_ne!(original, graph_hash(false, "dep", [2; 32]));
}

#[test]
fn built_executable_identity_hashes_all_bytes() {
    let path = std::path::Path::new(env!("CARGO_BIN_EXE_fluxc"));
    let started = std::time::Instant::now();
    let mut bytes = std::fs::read(path).unwrap();
    let expected = flux_hir::flib::digest(&bytes);
    eprintln!(
        "compiler identity: {} bytes read and SHA-256 in {:?}",
        bytes.len(),
        started.elapsed()
    );
    // A dirty compiler rebuild changes executable content even at the same git commit.
    let last = bytes.last_mut().unwrap();
    *last ^= 1;
    assert_ne!(expected, flux_hir::flib::digest(&bytes));
}
