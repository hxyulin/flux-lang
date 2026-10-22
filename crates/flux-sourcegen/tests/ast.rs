//! Checks that the generated AST is fresh and that the parser's trees match
//! the grammar.

#[path = "../../flux-syntax/tests/support/corpus.rs"]
mod corpus;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use corpus::Budget;
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
    let results = corpus::run_files("checked against the grammar", &files, None, check_file);
    let unclean: Vec<String> = (results.iter())
        .filter(|(_, problems)| problems.is_none())
        .map(|(path, _)| format!("{} has syntax errors", path.display()))
        .collect();
    assert!(unclean.is_empty(), "\n{}", unclean.join("\n"));
    report(results);
}

/// The same check over every file without syntax errors under
/// `FLUX_CORPUS`, and that none takes too long to parse and check for its
/// size.
#[test]
#[ignore = "needs FLUX_CORPUS"]
fn corpus_trees_match_grammar() {
    // Parsing a large file alone and checking it takes at most 80 ns/B
    // (October 2026).
    const BUDGET: Budget = Budget::new(200, 500.0);
    report(corpus::for_each_corpus_file(
        "checked against the grammar",
        BUDGET,
        check_file,
    ));
}

/// (node kind, unexpected child) to an example location.
type Problems = BTreeMap<(SyntaxKind, String), String>;

/// Fails with each problem found, at its first file by path.
fn report(results: Vec<(PathBuf, Option<Problems>)>) {
    let mut problems = Problems::new();
    for (key, at) in results.into_iter().flat_map(|(_, p)| p.unwrap_or_default()) {
        problems.entry(key).or_insert(at);
    }
    let report: Vec<String> = (problems.iter())
        .map(|((node, child), at)| format!("{node:?} has {child}, e.g. at {at}"))
        .collect();
    assert!(report.is_empty(), "\n{}", report.join("\n"));
}

/// The problems in the file at `path`, or None if it has syntax errors;
/// with the number of bytes read, for the corpus runner.
fn check_file(path: &Path, text: &str) -> (usize, Option<Problems>) {
    let parse = flux_syntax::parse_source_file(text);
    if !parse.errors().is_empty() {
        return (text.len(), None);
    }
    let mut problems = Problems::new();
    for node in parse.root().descendants() {
        check_node(node, path, &mut problems);
    }
    (text.len(), Some(problems))
}

fn check_node(node: SyntaxNode<'_>, path: &Path, problems: &mut Problems) {
    let kind = node.kind();
    let at = || format!("{}@{:?}", path.display(), node.text_range());
    if !allowed(kind, None) {
        let key = (kind, "no grammar rule".to_owned());
        problems.entry(key).or_insert_with(at);
        return;
    }
    for child in node.children_with_tokens() {
        if let SyntaxElement::Token(token) = child
            && (token.kind().is_trivia() || kind == SyntaxKind::TOKEN_TREE)
        {
            continue;
        }
        if !allowed(kind, Some(child.kind())) {
            let key = (kind, format!("{:?}", child.kind()));
            problems.entry(key).or_insert_with(at);
        }
    }
}

/// Whether `node` has a rule, without `child`; with it, whether the rule
/// mentions that kind of child node or token. Each thread looks each pair
/// up once.
fn allowed(node: SyntaxKind, child: Option<SyntaxKind>) -> bool {
    static RULES: OnceLock<BTreeMap<String, NodeDef>> = OnceLock::new();
    thread_local! {
        static ALLOWED: RefCell<HashMap<(SyntaxKind, Option<SyntaxKind>), bool>> =
            RefCell::new(HashMap::new());
    }
    let rules = RULES.get_or_init(|| {
        let nodes = load().nodes.into_iter();
        nodes.map(|n| (kind_name(&n.name), n)).collect()
    });
    ALLOWED.with_borrow_mut(|allowed| {
        *allowed.entry((node, child)).or_insert_with(|| {
            let Some(rule) = rules.get(&format!("{node:?}")) else {
                return false;
            };
            child.is_none_or(|child| {
                let name = format!("{child:?}");
                rule.child_nodes.contains(&name) || rule.child_tokens.contains(&name)
            })
        })
    })
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
