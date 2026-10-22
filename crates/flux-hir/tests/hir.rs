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

/// The same over every file under `FLUX_CORPUS`.
#[test]
#[ignore = "needs FLUX_CORPUS"]
fn corpus_lowering_never_panics() {
    for_each_corpus_file("lowered", |path, text| {
        flux_hir::lower_text_bare(path, text);
    });
}

/// Runs `f` on every `.rs` file under `FLUX_CORPUS`, on every core, and
/// fails with the files it panicked on; prints the slowest.
fn for_each_corpus_file(what: &str, f: impl Fn(PathBuf, &str) + Sync) {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    let root = std::env::var_os("FLUX_CORPUS").expect("set FLUX_CORPUS");
    let mut files = Vec::new();
    let mut stack = vec![PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    let next = AtomicUsize::new(0);
    let times: Mutex<Vec<(Duration, &Path)>> = Mutex::new(Vec::new());
    let panicked: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let start = Instant::now();
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                while let Some(path) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let Ok(text) = fs::read_to_string(path) else {
                        continue;
                    };
                    let file = Instant::now();
                    let run = || f(path.clone(), &text);
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).is_err() {
                        panicked.lock().unwrap().push(path.display().to_string());
                    }
                    times.lock().unwrap().push((file.elapsed(), path));
                }
            });
        }
    });
    let mut times = times.into_inner().unwrap();
    times.sort_by(|a, b| b.cmp(a));
    let total: Duration = times.iter().map(|t| t.0).sum();
    println!(
        "{what} {} files in {:.2?} ({total:.2?} on {threads} threads); slowest:",
        times.len(),
        start.elapsed()
    );
    for (time, path) in times.iter().take(5) {
        println!("{time:>12.2?}  {}", path.display());
    }
    let panicked = panicked.into_inner().unwrap();
    assert!(panicked.is_empty(), "panicked on:\n{}", panicked.join("\n"));
}
