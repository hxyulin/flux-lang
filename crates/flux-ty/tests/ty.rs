//! Snapshot tests for type inference.
//!
//! Every `tests/data/ok/*.rs` must type-check without diagnostics (and
//! compile with rustc), and every `err/*.rs` must report at least one;
//! each has a `.ty` file next to it with every function's signature and
//! binding types, then the diagnostics. Run with `UPDATE_EXPECT=1` to
//! write missing or changed snapshots, then review the diff.

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
        assert!(
            parse.errors().is_empty() && hir.diagnostics.is_empty(),
            "{} must parse and lower cleanly:\n{:#?}\n{:#?}",
            path.display(),
            parse.errors(),
            hir.diagnostics
        );
        let typed = flux_ty::check_file(&hir);
        let actual = typed.debug_dump(&hir);
        if typed.diagnostics.is_empty() == expect_errors {
            failures.push(format!(
                "{}: expected {}diagnostics\n{actual}",
                path.display(),
                if expect_errors { "" } else { "no " },
            ));
        }
        let snapshot = path.with_extension("ty");
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

/// `core`'s signatures are resolved once per process and a program's
/// after them; that gives what resolving everything again does.
#[test]
fn shared_core_signatures() {
    for path in fixtures("ok").into_iter().chain(fixtures("err")) {
        let text = fs::read_to_string(&path).unwrap();
        let (_, _, hir) = flux_hir::lower_text(&path, &text);
        let (shared, mut shared_diagnostics) = flux_ty::signatures(&hir.items);
        let (all, mut all_diagnostics) = flux_ty::signatures_checked(&hir.items, true);
        let name = path.display();
        assert_eq!(format!("{shared:?}"), format!("{all:?}"), "{name}");
        flux_hir::Diagnostic::sort(&mut shared_diagnostics);
        flux_hir::Diagnostic::sort(&mut all_diagnostics);
        assert_eq!(shared_diagnostics, all_diagnostics, "{name}");
    }
}

fn main_error(text: &str) -> Option<String> {
    let (_, _, hir) = flux_hir::lower_text("main.rs", text);
    let (sigs, _) = flux_ty::signatures(&hir.items);
    flux_ty::check_main(&hir.items, &sigs).map(|d| d.message)
}

#[test]
fn main_function() {
    assert_eq!(main_error("fn main() {}"), None);
    assert_eq!(main_error("fn main() -> ! { loop {} }"), None);
    assert_eq!(
        main_error("fn f() {}").as_deref(),
        Some("`main` function not found in crate `main`")
    );
    assert_eq!(
        main_error("fn main(x: i32) {}").as_deref(),
        Some("`main` function cannot take parameters")
    );
    assert_eq!(
        main_error("fn main() -> i32 { 0 }").as_deref(),
        Some("`main` has invalid return type `i32`")
    );
    assert_eq!(
        main_error("unsafe fn main() {}").as_deref(),
        Some("`main` function is not allowed to be `unsafe`")
    );
}

/// Type checking never panics on anything lowering produces: every prefix
/// of every parser fixture.
#[test]
fn checking_never_panics() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-syntax/tests/data/parser");
    for sub in ["ok", "err"] {
        for entry in fs::read_dir(dir.join(sub)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            for (i, _) in text.char_indices() {
                let (_, _, hir) = flux_hir::lower_text_bare("fuzz.rs", &text[..i]);
                flux_ty::check_file(&hir);
            }
        }
    }
}

/// The same over every file under `FLUX_CORPUS`.
#[test]
#[ignore = "needs FLUX_CORPUS"]
fn corpus_checking_never_panics() {
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
                let (_, _, hir) = flux_hir::lower_text_bare(path, &text);
                flux_ty::check_file(&hir);
                files += 1;
            }
        }
    }
    println!("checked {files} files");
}

/// `core`'s bodies are lowered and checked only where programs reach
/// them, so check all of them here.
#[test]
fn core_is_clean() {
    let (_, _, hir) = flux_hir::lower_text("main.rs", "fn main() {}\n");
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    for (id, f) in hir.items.functions.iter() {
        if f.has_body && !hir.is_program(id) {
            let lowered = hir.lower_core_body(id);
            assert!(lowered.diagnostics.is_empty(), "{:?}", lowered.diagnostics);
        }
    }
    let typed = flux_ty::check_file_all(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
}
