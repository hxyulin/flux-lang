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

    let mut bytes = 0usize;
    let mut clean = 0usize;
    let mut failures = Vec::new();
    let mut messages: HashMap<String, (usize, String)> = HashMap::new();
    let start = Instant::now();
    for path in &files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        bytes += text.len();
        let result = panic::catch_unwind(|| flux_syntax::parse_source_file(&text));
        let Ok(parse) = result else {
            failures.push(format!("{}: parser panicked", path.display()));
            continue;
        };
        if let Err(message) = parse.tree().validate() {
            failures.push(format!("{}: {message}", path.display()));
        } else if parse.root().text() != text {
            failures.push(format!("{}: tree does not round-trip", path.display()));
        }
        if parse.errors().is_empty() {
            clean += 1;
        } else {
            // Count only the first error per file; later ones are often
            // knock-on effects of the first.
            let error = &parse.errors()[0];
            let entry = messages
                .entry(error.message.clone())
                .or_insert_with(|| (0, format!("{}@{:?}", path.display(), error.range)));
            entry.0 += 1;
        }
    }
    let elapsed = start.elapsed();

    let mut messages: Vec<_> = messages.into_iter().collect();
    messages.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    println!(
        "{} files, {:.1} MiB in {elapsed:.2?} ({:.1} MiB/s); {clean} without errors ({:.1}%)",
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
