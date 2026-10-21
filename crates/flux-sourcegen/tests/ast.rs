//! Checks that the generated AST is fresh and that the parser's trees match
//! the grammar.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use flux_sourcegen::{NodeDef, kind_name, load, syntax_dir};
use flux_syntax::{SyntaxElement, SyntaxKind, SyntaxNode};

/// Fails when `src/ast/generated.rs` differs from what the grammar gives;
/// with `UPDATE_EXPECT=1`, rewrites it.
#[test]
fn generated_ast_is_fresh() {
    let code = flux_sourcegen::generate_ast();
    let path = flux_sourcegen::generated_path();
    let current = fs::read_to_string(&path).unwrap_or_default();
    if current != code {
        if std::env::var_os("UPDATE_EXPECT").is_some() {
            fs::write(&path, code).unwrap();
        } else {
            panic!(
                "{} is out of date with flux.ungram (run `cargo run -p flux-sourcegen`)",
                path.display()
            );
        }
    }
}

/// Every node in the fixtures and the workspace has a rule, and every
/// child it has is one its rule mentions.
#[test]
fn trees_match_grammar() {
    let root = syntax_dir().join("../..").canonicalize().unwrap();
    let mut files = Vec::new();
    collect_rs_files(&root.join("crates"), &mut files);
    collect_rs_files(&root.join("examples"), &mut files);
    let err_dir = syntax_dir().join("tests/data/parser/err");
    files.retain(|path| !path.starts_with(&err_dir));
    check_files(&files, true);
}

/// The same check over every file without syntax errors under
/// `FLUX_CORPUS`.
#[test]
#[ignore = "needs FLUX_CORPUS"]
fn corpus_trees_match_grammar() {
    let root = std::env::var_os("FLUX_CORPUS").expect("set FLUX_CORPUS");
    let mut files = Vec::new();
    collect_rs_files(Path::new(&root), &mut files);
    check_files(&files, false);
}

fn check_files(files: &[PathBuf], require_clean: bool) {
    let model = load();
    let rules: BTreeMap<String, &NodeDef> = model
        .nodes
        .iter()
        .map(|n| (kind_name(&n.name), n))
        .collect();
    // (node kind, unexpected child) to an example location.
    let mut problems: BTreeMap<(String, String), String> = BTreeMap::new();
    for path in files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let parse = flux_syntax::parse_source_file(&text);
        if !parse.errors().is_empty() {
            assert!(!require_clean, "{} has syntax errors", path.display());
            continue;
        }
        for node in parse.root().descendants() {
            check_node(node, &rules, path, &mut problems);
        }
    }
    let report: Vec<String> = problems
        .iter()
        .map(|((node, child), at)| format!("{node} has {child}, e.g. at {at}"))
        .collect();
    assert!(report.is_empty(), "\n{}", report.join("\n"));
}

fn check_node(
    node: SyntaxNode<'_>,
    rules: &BTreeMap<String, &NodeDef>,
    path: &Path,
    problems: &mut BTreeMap<(String, String), String>,
) {
    let kind = format!("{:?}", node.kind());
    let at = || format!("{}@{:?}", path.display(), node.text_range());
    let Some(rule) = rules.get(&kind) else {
        problems
            .entry((kind, "no grammar rule".to_owned()))
            .or_insert_with(at);
        return;
    };
    for child in node.children_with_tokens() {
        let child_kind = format!("{:?}", child.kind());
        let allowed = match child {
            SyntaxElement::Node(_) => rule.child_nodes.contains(&child_kind),
            SyntaxElement::Token(token) => {
                token.kind().is_trivia()
                    || node.kind() == SyntaxKind::TOKEN_TREE
                    || rule.child_tokens.contains(&child_kind)
            }
        };
        if !allowed {
            problems
                .entry((kind.clone(), child_kind))
                .or_insert_with(at);
        }
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
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
