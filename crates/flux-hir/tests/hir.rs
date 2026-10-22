//! Snapshot tests for HIR lowering.
//!
//! Every `tests/data/ok/*.rs` must lower without diagnostics, and every
//! `err/*.rs` with at least one; each has a `.hir` file next to it with
//! the expected output. Run with `UPDATE_EXPECT=1` to write missing or
//! changed snapshots, then review the diff.

#[path = "../../flux-syntax/tests/support/corpus.rs"]
mod corpus;

use std::fs;
use std::path::{Path, PathBuf};

use corpus::Budget;

fn fixtures(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(dir);
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures in {}", dir.display());
    files
}

fn check_dir(name: &str, expect_errors: bool) {
    let update = std::env::var_os("UPDATE_EXPECT").is_some();
    let mut failures = Vec::new();
    for path in fixtures(name) {
        let text = fs::read_to_string(&path).unwrap();
        let (db, root, hir) = flux_hir::lower_text(&path, &text);
        let parse = &db.file(root).parse;
        assert!(
            parse.errors().is_empty(),
            "{} has syntax errors:\n{:#?}",
            path.display(),
            parse.errors()
        );
        let actual = hir.debug_dump();
        if hir.diagnostics.is_empty() == expect_errors {
            failures.push(format!(
                "{}: expected {}diagnostics\n{actual}",
                path.display(),
                if expect_errors { "" } else { "no " },
            ));
        }
        let snapshot = path.with_extension("hir");
        match fs::read_to_string(&snapshot) {
            Ok(expected) if expected == actual => {}
            _ if update => fs::write(&snapshot, &actual).unwrap(),
            Ok(_) => failures.push(format!(
                "{} differs from {} (rerun with UPDATE_EXPECT=1 to update)",
                path.display(),
                snapshot.display()
            )),
            Err(_) => failures.push(format!(
                "{} is missing (rerun with UPDATE_EXPECT=1 to create it)",
                snapshot.display()
            )),
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

#[test]
fn ok_fixtures() {
    check_dir("ok", false);
}

#[test]
fn err_fixtures() {
    check_dir("err", true);
}

/// Lowering never panics, whatever the parser produced: every prefix of
/// every parser fixture stands in for a file being typed.
#[test]
fn lowering_never_panics() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-syntax/tests/data/parser");
    for sub in ["ok", "err"] {
        for entry in fs::read_dir(dir.join(sub)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            for (i, _) in text.char_indices() {
                flux_hir::lower_text_bare("fuzz.rs", &text[..i]);
            }
        }
    }
}

/// The same over every file under `FLUX_CORPUS` (with the files of its
/// modules), and none takes too long for its size.
#[test]
#[ignore = "needs FLUX_CORPUS"]
fn corpus_lowering_never_panics() {
    // Lowering a large file alone takes at most 50 ns/B (October 2026).
    const BUDGET: Budget = Budget::new(200, 1000.0);
    corpus::for_each_corpus_file("lowered", BUDGET, |path, text| {
        let (db, root, _) = flux_hir::lower_text_bare(path, text);
        (bytes_read(&db, root), ())
    });
}

/// The bytes of the files lowering `root` read: it and its modules' files.
fn bytes_read(db: &flux_hir::SourceDb, root: flux_hir::FileId) -> usize {
    (db.files())
        .filter(|&(id, _)| id >= root)
        .map(|(_, file)| file.text().len())
        .sum()
}

/// These actual compiler modules keep their test-only code in the input.
/// Filtering must happen before resolving its imports, expanding macros,
/// or lowering test-only associated functions.
#[test]
fn conditional_compilation_of_compiler_sources() {
    for (path, text, absent) in [
        (
            "syntax_kind.rs",
            include_str!("../../flux-syntax/src/syntax_kind.rs"),
            &["keywords", "pack_word_is_little_endian"][..],
        ),
        (
            "interval.rs",
            include_str!("../../flux-mir/src/interval.rs"),
            &["runs", "insert"][..],
        ),
    ] {
        let source = format!("mod compiler_source {{\n{text}\n}}\nfn main() {{}}\n");
        let (_, _, hir) = flux_hir::lower_text(path, &source);
        // The exported `T!` macro still has an independently unsupported
        // literal `[$]` matcher. This test concerns cfg filtering only.
        if path == "interval.rs" {
            assert!(hir.diagnostics.is_empty(), "{path}: {:?}", hir.diagnostics);
        } else {
            assert!(
                hir.diagnostics
                    .iter()
                    .all(|d| d.message == "expected identifier, found `$`"),
                "{path}: {:?}",
                hir.diagnostics
            );
        }
        assert!(
            hir.items
                .modules
                .iter()
                .all(|(_, m)| m.name.as_str() != "tests")
        );
        for (id, function) in hir.items.functions.iter() {
            if hir.is_program(id) {
                assert!(
                    !absent.contains(&function.name.as_str()),
                    "{path}: {}",
                    function.name
                );
            }
        }
    }
}
