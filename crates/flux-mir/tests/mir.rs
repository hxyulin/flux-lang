//! Snapshot tests for MIR building and the initialization check.
//!
//! Every `tests/data/ok/*.rs` must build without diagnostics (and compile
//! with rustc), and every `err/*.rs` must report at least one; each has a
//! `.mir` file next to it with the MIR and diagnostics. Run with
//! `UPDATE_EXPECT=1` to write missing or changed snapshots, then review
//! the diff.

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
        let (db, root, hir) = flux_hir::lower_text(&path, &text);
        let parse = &db.file(root).parse;
        let typed = flux_mir::check_file(&hir);
        assert!(
            parse.errors().is_empty() && hir.diagnostics.is_empty() && typed.diagnostics.is_empty(),
            "{} must parse, lower and type-check cleanly:\n{:#?}\n{:#?}\n{:#?}",
            path.display(),
            parse.errors(),
            hir.diagnostics,
            typed.diagnostics
        );
        let mir = flux_mir::build_file(&hir, &typed);
        let actual = mir.debug_dump(&hir);
        if mir.diagnostics.is_empty() == expect_errors {
            failures.push(format!(
                "{}: expected {}diagnostics\n{actual}",
                path.display(),
                if expect_errors { "" } else { "no " },
            ));
        }
        let snapshot = path.with_extension("mir");
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

/// The errors type checking reports for `text`, as `line:column:
/// message`.
fn type_errors(text: &str) -> Vec<String> {
    let (_db, _, hir) = flux_hir::lower_text("main.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:#?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    typed
        .diagnostics
        .iter()
        .map(|d| {
            let before = &text[..d.range.start() as usize];
            let line = before.matches('\n').count() + 1;
            let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
            format!("{line}:{column}: {}", d.message)
        })
        .collect()
}

/// Array lengths that name constants need their values while types are
/// checked, so their evaluation errors stop type checking, as in rustc.
#[test]
fn array_length_errors() {
    let text = "\
const fn half(n: usize) -> usize {
    n / 0
}
const N: usize = half(4);
fn f(_: [u8; N]) {}
fn main() {}
";
    assert_eq!(
        type_errors(text),
        ["4:18: attempt to divide `4_usize` by zero"]
    );
    let text = "\
const A: usize = B;
const B: usize = A;
fn f(_: [u8; A]) {}
fn main() {}
";
    assert_eq!(
        type_errors(text),
        [
            "1:18: cycle detected when const-evaluating `A`",
            "2:18: cycle detected when const-evaluating `A`"
        ]
    );
    // A length whose constant's own value names it.
    let text = "\
const A: usize = [0u8; A].len();
fn main() {
    let _: [u8; A] = [];
}
";
    assert_eq!(
        type_errors(text),
        ["1:1: cycle detected when evaluating type-level constant `A`"]
    );
}
