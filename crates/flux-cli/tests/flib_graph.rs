#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use flux_cli::flib::semantic::{
    CheckedCrate, Dependency, DependencyState, resolve_paths, write_checked,
};
use flux_hir::flib::CrateIndices;
use flux_intern::Symbol;

#[test]
fn sibling_artifacts_resolve_transitive_hashes_before_semantic_loading() {
    let initial = DependencyState::new().unwrap();
    let mut db = flux_hir::SourceDb::new();
    let a = db.add("a.rs", "pub const N: u32 = 7; pub fn id<T>(x:T)->T{x}");
    let b = db.add(
        "b.rs",
        "pub const M: u32 = a::N + 1; pub fn value()->u32{ a::id(M) }",
    );
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            flux_hir::SourceCrate {
                root: a,
                name: Symbol::intern("a"),
                dependencies: Vec::new(),
            },
            flux_hir::SourceCrate {
                root: b,
                name: Symbol::intern("b"),
                dependencies: vec![(Symbol::intern("a"), 0)],
            },
        ],
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let b_id = hir.items.main_crate();
    let a_id = flux_hir::CrateId(b_id.0 - 1);
    let a_hash = [41; 32];
    let b_hash = [42; 32];
    let a_bytes = write_checked(CheckedCrate {
        items: &hir.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: a_id,
        hash: a_hash,
        dependencies: &initial.dependencies,
        object: None,
        freshness: None,
    })
    .unwrap();
    let mut dependencies = initial.dependencies.clone();
    dependencies.push(Dependency {
        krate: a_id,
        hash: a_hash,
        indices: CrateIndices::collect(&hir.items, a_id),
    });
    let b_bytes = write_checked(CheckedCrate {
        items: &hir.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: b_id,
        hash: b_hash,
        dependencies: &dependencies,
        object: None,
        freshness: None,
    })
    .unwrap();
    let scratch = scratch::Scratch::new(std::env::temp_dir().join("flux-flib-graph"));
    let directory = scratch.path();
    let a_path = directory.join("renamed-a.flib");
    let b_path = directory.join("b.flib");
    std::fs::write(&a_path, a_bytes).unwrap();
    std::fs::write(&b_path, b_bytes).unwrap();
    let ordered = resolve_paths(std::slice::from_ref(&b_path), &initial.dependencies).unwrap();
    assert_eq!(
        ordered
            .iter()
            .map(|r| r.manifest.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let state = initial.load(std::slice::from_ref(&b_path)).unwrap();
    let loaded_b = state
        .dependencies
        .iter()
        .find(|d| d.hash == b_hash)
        .unwrap();
    let value = state
        .items
        .consts
        .iter()
        .find(|(_, c)| {
            state.items.modules[c.loc.module].krate == loaded_b.krate
                && c.name.is_some_and(|n| n.as_str() == "M")
        })
        .unwrap()
        .0;
    assert_eq!(
        state.mir.const_values.items.get(value),
        Some(&flux_mir::eval::Value::Int {
            bits: 8,
            ty: flux_ty::IntTy::U32
        })
    );
    for dependency in state
        .dependencies
        .iter()
        .filter(|d| d.hash == a_hash || d.hash == b_hash)
    {
        for &file in &dependency.indices.files {
            assert!(
                state
                    .db
                    .file(flux_hir::FileId(file))
                    .parse
                    .tree()
                    .text()
                    .is_empty()
            );
        }
    }
    std::fs::remove_file(a_path).unwrap();
    assert!(
        resolve_paths(
            &[b_path],
            &state.dependencies[..state.dependencies.len() - 2]
        )
        .unwrap_err()
        .contains("missing fluxlib dependency")
    );
}
