//! Compare acceptance and rejection of block exits with rustc.
use std::path::Path;
use std::process::Command;

#[test]
fn block_exit_acceptance_matches_rustc() {
    let fixtures = [
        ("../flux-ty/tests/data/ok/labeled_blocks.rs", true),
        ("../flux-hir/tests/data/err/labeled_block_targets.rs", false),
        ("../flux-ty/tests/data/err/labeled_block_values.rs", false),
        ("tests/data/err/labeled_block_init.rs", false),
        ("tests/data/err/labeled_block_borrow.rs", false),
        ("tests/data/err/labeled_block_move.rs", false),
    ];
    let dir = std::env::temp_dir().join(format!("flux-block-labels-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (fixture, accepted) in fixtures {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture);
        let source = std::fs::read_to_string(&path).unwrap();
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        // MIR requires diagnostic-free lowering and type checking. Follow the
        // driver pipeline rather than lowering invalid targets in debug builds.
        let typed = hir
            .diagnostics
            .is_empty()
            .then(|| flux_mir::check_file(&hir));
        let mir = typed
            .as_ref()
            .filter(|typed| typed.diagnostics.is_empty())
            .map(|typed| flux_mir::build_file(&hir, typed));
        let flux_ok = mir.as_ref().is_some_and(|mir| mir.diagnostics.is_empty());
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "{}: {}",
            path.display(),
            String::from_utf8_lossy(&rustc.stderr)
        );
        assert_eq!(
            flux_ok,
            accepted,
            "{}: {:?} {:?} {:?}",
            path.display(),
            hir.diagnostics,
            typed.as_ref().map(|typed| &typed.diagnostics),
            mir.as_ref().map(|mir| &mir.diagnostics)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn label_names_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-label-names-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.rs");
    for (edition, source, accepted) in [
        (
            flux_syntax::Edition::E2021,
            "fn main() { 'gen: { break 'gen; } }",
            true,
        ),
        (
            flux_syntax::Edition::E2024,
            "fn main() { 'gen: { break 'gen; } }",
            false,
        ),
        (
            flux_syntax::Edition::E2024,
            "fn main() { 'static: { break 'static; } }",
            false,
        ),
        (
            flux_syntax::Edition::E2024,
            "fn main() { '_: { break '_; } }",
            false,
        ),
    ] {
        std::fs::write(&path, source).unwrap();
        let mut db = flux_hir::SourceDb::new();
        let root = db.add_with_edition(&path, source, edition);
        let hir = flux_hir::lower(&mut db, root, flux_intern::Symbol::intern("main"));
        let rustc = Command::new("rustc")
            .arg(if edition == flux_syntax::Edition::E2021 {
                "--edition=2021"
            } else {
                "--edition=2024"
            })
            .args(["--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "{source}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        assert_eq!(
            hir.diagnostics.is_empty() && db.syntax_errors().is_empty(),
            accepted,
            "{source}: {:?}",
            hir.diagnostics
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
