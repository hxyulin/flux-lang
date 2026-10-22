//! Parses an external corpus of Rust files, such as `~/.cargo/registry/src`
//! or a large project, to find panics, malformed trees and unsupported syntax.
//!
//! ```sh
//! FLUX_CORPUS=~/.cargo/registry/src cargo test -p flux-syntax --release \
//!     --test corpus -- --ignored --nocapture
//! ```
//!
//! Panics and malformed or lossy trees fail the test. Syntax errors are expected, since
//! flux supports only a subset of Rust; the most common messages are
//! printed as a guide to what to support next.

use std::collections::HashMap;
use std::fs;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

#[test]
#[ignore = "needs FLUX_CORPUS"]
fn corpus() {
    let Some(root) = std::env::var_os("FLUX_CORPUS") else {
        panic!("set FLUX_CORPUS to a directory of Rust files");
    };
    let mut files = Vec::new();
    collect(Path::new(&root), &mut files);
    files.sort();

    let next = AtomicUsize::new(0);
    let bytes = AtomicUsize::new(0);
    let clean = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let messages: Mutex<HashMap<String, (usize, String)>> = Mutex::new(HashMap::new());
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let start = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                while let Some(path) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let Ok(text) = fs::read_to_string(path) else {
                        continue;
                    };
                    bytes.fetch_add(text.len(), Ordering::Relaxed);
                    let result = panic::catch_unwind(|| flux_syntax::parse_source_file(&text));
                    let Ok(parse) = result else {
                        let failure = format!("{}: parser panicked", path.display());
                        failures.lock().unwrap().push(failure);
                        continue;
                    };
                    if let Err(message) = parse.tree().validate() {
                        let failure = format!("{}: {message}", path.display());
                        failures.lock().unwrap().push(failure);
                    } else if parse.root().text() != text {
                        let failure = format!("{}: tree does not round-trip", path.display());
                        failures.lock().unwrap().push(failure);
                    }
                    if parse.errors().is_empty() {
                        clean.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // Count only the first error per file; later ones are
                        // often knock-on effects of the first.
                        let error = &parse.errors()[0];
                        let example = format!("{}@{:?}", path.display(), error.range);
                        let mut messages = messages.lock().unwrap();
                        let entry = messages
                            .entry(error.message.clone())
                            .or_insert_with(|| (0, example.clone()));
                        entry.0 += 1;
                        // The first by path, whichever thread found it.
                        if example < entry.1 {
                            entry.1 = example;
                        }
                    }
                }
            });
        }
    });
    let elapsed = start.elapsed();

    let (bytes, clean) = (bytes.into_inner(), clean.into_inner());
    let failures = failures.into_inner().unwrap();
    let mut messages: Vec<_> = messages.into_inner().unwrap().into_iter().collect();
    messages.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    println!(
        "{} files, {:.1} MiB in {elapsed:.2?} on {threads} threads ({:.1} MiB/s); \
         {clean} without errors ({:.1}%)",
        files.len(),
        bytes as f64 / (1 << 20) as f64,
        bytes as f64 / (1 << 20) as f64 / elapsed.as_secs_f64(),
        100.0 * clean as f64 / files.len().max(1) as f64,
    );
    println!("most common first errors:");
    for (message, (count, example)) in messages.iter().take(25) {
        println!("{count:>7}  {message}\n         e.g. {example}");
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}
