//! Parses an external corpus of Rust files, such as `~/.cargo/registry/src`
//! or a large project, to find panics, malformed trees, unsupported syntax
//! and files that take too long for their size.
//!
//! ```sh
//! FLUX_CORPUS=~/.cargo/registry/src cargo test -p flux-syntax --release \
//!     --test corpus -- --ignored --nocapture
//! ```
//!
//! Panics, malformed or lossy trees and slow files fail the test (see
//! `support/corpus.rs` for the time budget). Syntax errors are expected,
//! since flux supports only a subset of Rust; the most common messages are
//! printed as a guide to what to support next.

#[path = "support/corpus.rs"]
mod corpus;

use std::collections::HashMap;

use corpus::Budget;

/// Parsing a large file alone takes at most 6 ns/B (October 2026).
const BUDGET: Budget = Budget::new(200, 100.0);

#[test]
#[ignore = "needs FLUX_CORPUS"]
fn corpus() {
    let results = corpus::for_each_corpus_file("parsed", BUDGET, |_, text| {
        let parse = flux_syntax::parse_source_file(text);
        let failure = if let Err(message) = parse.tree().validate() {
            Some(message)
        } else if parse.root().text() != text {
            Some("tree does not round-trip".to_owned())
        } else {
            None
        };
        // Only the first error per file; later ones are often knock-on
        // effects of the first.
        let error = parse.errors().first().map(|e| (e.message.clone(), e.range));
        (text.len(), (failure, error))
    });

    let mut failures = Vec::new();
    let mut clean = 0;
    let mut messages: HashMap<String, (usize, String)> = HashMap::new();
    for (path, (failure, error)) in &results {
        if let Some(failure) = failure {
            failures.push(format!("{}: {failure}", path.display()));
        }
        let Some((message, range)) = error else {
            clean += 1;
            continue;
        };
        // Files come in order, so the example is the first by path.
        let entry = messages
            .entry(message.clone())
            .or_insert_with(|| (0, format!("{}@{range:?}", path.display())));
        entry.0 += 1;
    }
    let mut messages: Vec<_> = messages.into_iter().collect();
    messages.sort_by(|(a, (m, _)), (b, (n, _))| n.cmp(m).then(a.cmp(b)));
    println!(
        "{clean} without errors ({:.1}%)",
        100.0 * clean as f64 / results.len().max(1) as f64,
    );
    println!("most common first errors:");
    for (message, (count, example)) in messages.iter().take(25) {
        println!("{count:>7}  {message}\n         e.g. {example}");
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
