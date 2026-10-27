//! Crate environments are immutable inputs, not process-global mutations.
use flux_hir::{FileId, SourceCrate, SourceDb};
use flux_intern::Symbol;
use std::collections::BTreeMap;

#[test]
fn explicit_empty_environment_hides_process_variables() {
    // PATH is present for running this test and must not leak into an explicit
    // empty compilation environment. No process environment is mutated.
    assert!(std::env::var_os("PATH").is_some());
    let mut db = SourceDb::new();
    let root = db.add(
        "empty-env.rs",
        "const VALUE:&str=env!(\"PATH\");fn main(){}",
    );
    db.set_crate_env(root, BTreeMap::new()).unwrap();
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("empty_env"));
    assert!(
        hir.diagnostics
            .iter()
            .any(|d| d.message.contains("environment variable `PATH`")),
        "{:?}",
        hir.diagnostics
    );
    assert!(
        db.set_crate_env(FileId::CORE_ROOT, BTreeMap::new())
            .is_err()
    );
    assert!(db.set_crate_env(FileId::UNKNOWN, BTreeMap::new()).is_err());
}

#[test]
fn environments_are_separate_across_compilations() {
    let workers = (0..4)
        .map(|index| {
            std::thread::spawn(move || {
                let mut db = SourceDb::new();
                let root = db.add(
                    "same-source.rs",
                    "include!(env!(\"GENERATED_SOURCE\")); fn main(){}",
                );
                db.set_crate_env(
                    root,
                    BTreeMap::from([(
                        "GENERATED_SOURCE".into(),
                        format!("does-not-exist-{index}.rs"),
                    )]),
                )
                .unwrap();
                let hir = flux_hir::lower_sources(
                    &mut db,
                    &[SourceCrate {
                        root,
                        name: Symbol::intern("same"),
                        dependencies: Vec::new(),
                    }],
                );
                let text = format!("{:?}", hir.diagnostics);
                assert!(
                    text.contains(&format!("does-not-exist-{index}.rs")),
                    "{text}"
                );
                for other in 0..4 {
                    if other != index {
                        assert!(!text.contains(&format!("does-not-exist-{other}.rs")));
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
}
