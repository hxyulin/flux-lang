//! Snapshot tests for HIR lowering.
//!
//! Every `tests/data/ok/*.rs` must lower without diagnostics, and every
//! `err/*.rs` with at least one; each has a `.hir` file next to it with
//! the expected output. Run with `UPDATE_EXPECT=1` to write missing or
//! changed snapshots, then review the diff.

use std::fs;
use std::path::{Path, PathBuf};

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
        let parse = flux_syntax::parse_source_file(&text);
        assert!(
            parse.errors().is_empty(),
            "{} has syntax errors:\n{:#?}",
            path.display(),
            parse.errors()
        );
        let hir = flux_hir::lower_file(&parse);
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
                let parse = flux_syntax::parse_source_file(&text[..i]);
                flux_hir::lower_file(&parse);
            }
        }
    }
}

/// The same over every file under `FLUX_CORPUS`.
#[test]
#[ignore = "needs FLUX_CORPUS"]
fn corpus_lowering_never_panics() {
    let root = std::env::var_os("FLUX_CORPUS").expect("set FLUX_CORPUS");
    let mut stack = vec![PathBuf::from(root)];
    let mut files = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && let Ok(text) = fs::read_to_string(&path)
            {
                let parse = flux_syntax::parse_source_file(&text);
                flux_hir::lower_file(&parse);
                files += 1;
            }
        }
    }
    println!("lowered {files} files");
}
