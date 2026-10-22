//! Checks that the generated AST is fresh and that the parser's trees match
//! the grammar.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Checks `files` on every core.
fn check_files(files: &[PathBuf], require_clean: bool) {
    let model = load();
    let rules: BTreeMap<String, &NodeDef> = model
        .nodes
        .iter()
        .map(|n| (kind_name(&n.name), n))
        .collect();
    let next = AtomicUsize::new(0);
    let problems = Mutex::new(BTreeMap::new());
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                let mut checker = Checker {
                    rules: &rules,
                    allowed: HashMap::new(),
                    problems: BTreeMap::new(),
                };
                while let Some(path) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let Ok(text) = fs::read_to_string(path) else {
                        continue;
                    };
                    let parse = flux_syntax::parse_source_file(&text);
                    if !parse.errors().is_empty() {
                        assert!(!require_clean, "{} has syntax errors", path.display());
                        continue;
                    }
                    for node in parse.root().descendants() {
                        checker.check_node(node, path);
                    }
                }
                let mut problems = problems.lock().unwrap();
                for (key, at) in checker.problems {
                    let entry = problems.entry(key).or_insert_with(|| at.clone());
                    // The first by path, whichever thread found it.
                    if at < *entry {
                        *entry = at;
                    }
                }
            });
        }
    });
    let report: Vec<String> = problems
        .into_inner()
        .unwrap()
        .iter()
        .map(|((node, child), at)| format!("{node:?} has {child}, e.g. at {at}"))
        .collect();
    assert!(report.is_empty(), "\n{}", report.join("\n"));
}

/// One thread's check: whether each kind of child may appear in each
/// kind of node, looked up once.
struct Checker<'a> {
    rules: &'a BTreeMap<String, &'a NodeDef>,
    allowed: HashMap<(SyntaxKind, Option<SyntaxKind>), bool>,
    /// (node kind, unexpected child) to an example location.
    problems: BTreeMap<(SyntaxKind, String), String>,
}

impl Checker<'_> {
    fn check_node(&mut self, node: SyntaxNode<'_>, path: &Path) {
        let kind = node.kind();
        let at = || format!("{}@{:?}", path.display(), node.text_range());
        if !self.allowed(kind, None) {
            let key = (kind, "no grammar rule".to_owned());
            self.problems.entry(key).or_insert_with(at);
            return;
        }
        for child in node.children_with_tokens() {
            if let SyntaxElement::Token(token) = child
                && (token.kind().is_trivia() || kind == SyntaxKind::TOKEN_TREE)
            {
                continue;
            }
            if !self.allowed(kind, Some(child.kind())) {
                let key = (kind, format!("{:?}", child.kind()));
                self.problems.entry(key).or_insert_with(at);
            }
        }
    }

    /// Whether `node` has a rule, without `child`; with it, whether the
    /// rule mentions that kind of child node or token.
    fn allowed(&mut self, node: SyntaxKind, child: Option<SyntaxKind>) -> bool {
        let rules = self.rules;
        *self.allowed.entry((node, child)).or_insert_with(|| {
            let Some(rule) = rules.get(&format!("{node:?}")) else {
                return false;
            };
            child.is_none_or(|child| {
                let name = format!("{child:?}");
                rule.child_nodes.contains(&name) || rule.child_tokens.contains(&name)
            })
        })
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
