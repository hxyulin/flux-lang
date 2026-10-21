//! Snapshot tests for the parser.
//!
//! Every `tests/data/parser/ok/*.rs` must parse without errors and every
//! `err/*.rs` with at least one; each has a `.rast` file next to it with
//! the expected tree. Run with `UPDATE_EXPECT=1` to write missing or
//! changed snapshots, then review the diff.

use std::fs;
use std::path::{Path, PathBuf};

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/parser")
        .canonicalize()
        .unwrap()
}

fn fixtures(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures in {}", dir.display());
    files
}

/// The tree is structurally sound and its tokens, read through the tree,
/// spell out the input exactly.
fn assert_well_formed(parse: &flux_syntax::Parse, text: &str, what: &str) {
    if let Err(message) = parse.tree().validate() {
        panic!("{what}: {message}");
    }
    let mut spelled = String::with_capacity(text.len());
    spell(parse.root(), &mut spelled);
    assert_eq!(spelled, text, "{what} does not round-trip");
}

fn spell(node: flux_syntax::SyntaxNode<'_>, out: &mut String) {
    for element in node.children_with_tokens() {
        match element {
            flux_syntax::SyntaxElement::Node(child) => spell(child, out),
            flux_syntax::SyntaxElement::Token(token) => out.push_str(token.text()),
        }
    }
}

fn check_dir(name: &str, expect_errors: bool) {
    let update = std::env::var_os("UPDATE_EXPECT").is_some();
    let mut failures = Vec::new();
    for path in fixtures(&data_dir().join(name)) {
        let text = fs::read_to_string(&path).unwrap();
        let parse = flux_syntax::parse_source_file(&text);
        assert_well_formed(&parse, &text, &path.display().to_string());
        let has_errors = !parse.errors().is_empty();
        if has_errors != expect_errors {
            failures.push(format!(
                "{}: expected {}errors\n{}",
                path.display(),
                if expect_errors { "" } else { "no " },
                parse.debug_dump()
            ));
        }
        let actual = parse.debug_dump();
        let snapshot = path.with_extension("rast");
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

/// Every prefix of every fixture is a stand-in for a file being typed:
/// parsing must not panic and must reproduce the input exactly.
#[test]
fn prefixes_parse_losslessly() {
    for dir in ["ok", "err"] {
        for path in fixtures(&data_dir().join(dir)) {
            let text = fs::read_to_string(&path).unwrap();
            for (i, _) in text.char_indices() {
                let prefix = &text[..i];
                let parse = flux_syntax::parse_source_file(prefix);
                let what = format!("prefix of {} ending at byte {i}", path.display());
                assert_well_formed(&parse, prefix, &what);
            }
        }
    }
}

/// The parser's own workspace is real Rust inside the supported subset.
#[test]
fn workspace_sources_parse_without_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut files = Vec::new();
    collect_rs_files(&root.join("crates"), &mut files);
    let fixtures_dir = data_dir();
    files.retain(|path| !path.starts_with(&fixtures_dir));
    assert!(files.len() > 10, "found only {} source files", files.len());
    for path in files {
        let text = fs::read_to_string(&path).unwrap();
        let parse = flux_syntax::parse_source_file(&text);
        assert!(
            parse.errors().is_empty(),
            "{}:\n{:#?}",
            path.display(),
            parse.errors()
        );
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}
