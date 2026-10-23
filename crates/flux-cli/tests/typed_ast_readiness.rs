//! Full unchanged typed-AST source-graph READINESS, not public compilation,
//! native execution, or self-hosting acceptance. The public trait gate stays.
//! Flux loads dependency sources; rustc independently checks metadata using
//! rlibs built here from those same current sources, never building Flux.
//! Like tree_pipeline_selfhost, this graph is bounded to arm64 macOS, memchr
//! 2.8.3 through an explicit feature-free edition-2024 wrapper, and unicode-ident
//! 1.0.27 through its unchanged edition-2021 manifest. No Cargo feature/build
//! script/registry compilation support is implied.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

#[path = "support/typed_ast_readiness.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use flux_hir::{Diagnostic, SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// Copy every byte, including cfg(test) items, and retain the inventory for
/// comparison again after both compilers have inspected the assembled graph.
fn copy_tree(source: &Path, target: &Path, inventory: &mut Vec<(PathBuf, PathBuf)>) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination, inventory);
        } else {
            std::fs::copy(entry.path(), &destination).unwrap();
            inventory.push((entry.path(), destination));
        }
    }
}

fn assert_identical(inventory: &[(PathBuf, PathBuf)]) {
    for (original, copy) in inventory {
        assert_eq!(
            std::fs::read(original).unwrap(),
            std::fs::read(copy).unwrap(),
            "source graph changed: {}",
            original.display()
        );
    }
}

fn run(root: &Path, label: &str, command: &mut Command) {
    let output = command.output().unwrap();
    std::fs::write(root.join(format!("{label}.stdout")), &output.stdout).unwrap();
    std::fs::write(root.join(format!("{label}.stderr")), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn reference_library(
    root: &Path,
    name: &str,
    source: &str,
    edition: &str,
    deps: &[(&str, PathBuf)],
) -> PathBuf {
    let library = root.join(format!("lib{name}.rlib"));
    let mut command = Command::new("rustc");
    command
        .args([
            "--crate-type=rlib",
            "--crate-name",
            name,
            "--edition",
            edition,
        ])
        .arg(root.join(source))
        .arg("-L")
        .arg(root)
        .arg("-o")
        .arg(&library);
    for (name, path) in deps {
        command
            .arg("--extern")
            .arg(format!("{name}={}", path.display()));
    }
    run(root, name, &mut command);
    library
}

/// Exact identity, not a substring/count-only allowance for unsupported errors.
fn known_gate_only(diagnostics: &[Diagnostic], expected: &Diagnostic) -> bool {
    diagnostics == std::slice::from_ref(expected)
}

#[test]
fn readiness_rejects_an_actual_unrelated_type_error() {
    let root = std::env::temp_dir().join(format!(
        "flux-typed-ast-readiness-negative-{}",
        std::process::id()
    ));
    // Invalid source is compile-only. Preserve both compilers' diagnostics;
    // in particular, never call build_file with these unrelated type errors.
    let source = "trait AstNode<'t> {} fn main() { let _: bool = 1; }";
    write(&root, "negative.rs", source);
    let rustc = Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(root.join("negative.rs"))
        .output()
        .unwrap();
    std::fs::write(root.join("rustc.stderr"), &rustc.stderr).unwrap();
    assert!(!rustc.status.success());
    assert!(String::from_utf8_lossy(&rustc.stderr).contains("mismatched types"));
    let (db, file, hir) = flux_hir::lower_text(root.join("negative.rs"), source);
    assert!(db.syntax_errors().is_empty());
    assert!(hir.diagnostics.is_empty());
    let typed = flux_mir::check_file(&hir);
    record(&root, "type", &db, &typed.diagnostics);
    let owner = hir
        .items
        .traits
        .values()
        .find(|t| t.name.as_str() == "AstNode")
        .unwrap();
    let expected = Diagnostic {
        file,
        range: owner.generics.lifetimes[0].range,
        message: "lifetime parameters on traits are not supported yet".to_owned(),
    };
    assert!(typed.diagnostics.contains(&expected));
    assert_eq!(typed.diagnostics.len(), 2, "{:?}", typed.diagnostics);
    assert!(!known_gate_only(&typed.diagnostics, &expected));
}

fn record(root: &Path, label: &str, db: &SourceDb, diagnostics: &[Diagnostic]) {
    let mut text = format!("count={}\n", diagnostics.len());
    for diagnostic in diagnostics {
        text.push_str(&format!(
            "{}:{:?}: {}\n",
            db.file(diagnostic.file).path.display(),
            diagnostic.range,
            diagnostic.message
        ));
    }
    write(root, &format!("{label}.diagnostics"), &text);
}

fn analyze(root: &Path, consumer: &str) {
    let started = Instant::now();
    let mut db = SourceDb::new();
    let specs = [
        ("memchr-src/src/lib.rs", "memchr", vec![], Edition::E2024),
        (
            "unicode-ident-src/src/lib.rs",
            "unicode_ident",
            vec![],
            Edition::E2021,
        ),
        (
            "syntax/src/lib.rs",
            "syntax_complete",
            vec![("memchr", 0), ("unicode_ident", 1)],
            Edition::E2024,
        ),
        (
            consumer,
            "ast_readiness",
            vec![("syntax_complete", 2)],
            Edition::E2024,
        ),
    ];
    let sources: Vec<_> = specs
        .into_iter()
        .map(|(path, name, dependencies, edition)| SourceCrate {
            root: db.load_with_edition(&root.join(path), edition).unwrap(),
            name: Symbol::intern(name),
            dependencies: dependencies
                .into_iter()
                .map(|(name, index)| (Symbol::intern(name), index))
                .collect(),
        })
        .collect();
    let hir = flux_hir::lower_sources(&mut db, &sources);
    let syntax = db.syntax_errors();
    record(root, &format!("{consumer}.syntax"), &db, &syntax);
    record(root, &format!("{consumer}.hir"), &db, &hir.diagnostics);
    assert!(syntax.is_empty(), "{syntax:?}");
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    record(root, &format!("{consumer}.type"), &db, &typed.diagnostics);
    let owners: Vec<_> = hir
        .items
        .traits
        .values()
        .filter(|t| t.name.as_str() == "AstNode")
        .collect();
    assert_eq!(owners.len(), 1);
    let owner = owners[0];
    assert_eq!(owner.generics.lifetimes.len(), 1);
    let (file, _) = db
        .files()
        .find(|(_, file)| file.path == root.join("syntax/src/ast.rs"))
        .unwrap();
    let expected = Diagnostic {
        file,
        range: owner.generics.lifetimes[0].range,
        message: "lifetime parameters on traits are not supported yet".to_owned(),
    };
    // Tie the semantic owner/range back to the unchanged source declaration.
    let text = db.file(file).text();
    let start = text.find("pub trait AstNode<'t>").unwrap() + "pub trait AstNode<".len();
    assert_eq!(
        expected.range,
        flux_syntax::TextRange::new(start as u32, (start + 2) as u32)
    );
    assert!(
        known_gate_only(&typed.diagnostics, &expected),
        "{:?}",
        typed.diagnostics
    );
    assert!(!known_gate_only(&[], &expected));
    let mut unrelated = expected.clone();
    unrelated.message = "unrelated typed error".to_owned();
    assert!(!known_gate_only(&[unrelated.clone()], &expected));
    assert!(!known_gate_only(&[expected.clone(), unrelated], &expected));
    let mut wrong_location = expected.clone();
    wrong_location.file = sources[3].root;
    assert!(!known_gate_only(&[wrong_location], &expected));
    let mut wrong_range = expected.clone();
    wrong_range.range = flux_syntax::TextRange::new(0, 0);
    assert!(!known_gate_only(&[wrong_range], &expected));
    // Deliberate low-level prerequisite evidence, following trait_owner_regions:
    // ONLY this known declaration gate permits inspecting independent MIR. Keep
    // every original diagnostic untouched. Never send this result to codegen or
    // treat it as public acceptance; build_file normally requires clean types.
    let mir = flux_mir::build_file(&hir, &typed);
    record(root, &format!("{consumer}.mir"), &db, &mir.diagnostics);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    assert!(known_gate_only(&typed.diagnostics, &expected));
    eprintln!(
        "{consumer}: syntax=0 HIR=0 type=1 known gate MIR=0 ({:?})",
        started.elapsed()
    );
}

#[test]
fn full_unchanged_typed_ast_source_graph_readiness() {
    let started = Instant::now();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Preserve the complete reproducer and all diagnostics, including on failure.
    let root =
        std::env::temp_dir().join(format!("flux-typed-ast-readiness-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    eprintln!("typed AST readiness reproducer: {}", root.display());
    let metadata = Command::new("cargo")
        .current_dir(&repository)
        .args([
            "metadata",
            "--offline",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            "aarch64-apple-darwin",
        ])
        .output()
        .unwrap();
    assert!(
        metadata.status.success(),
        "{}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let mut inventory = Vec::new();
    for (name, version) in [("memchr", "2.8.3"), ("unicode-ident", "1.0.27")] {
        let package = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name && p["version"] == version)
            .unwrap();
        let manifest = Path::new(package["manifest_path"].as_str().unwrap());
        copy_tree(
            manifest.parent().unwrap(),
            &root.join(format!("{name}-src")),
            &mut inventory,
        );
    }
    copy_tree(
        &repository.join("crates/flux-syntax/src"),
        &root.join("syntax/src"),
        &mut inventory,
    );
    write(
        &root,
        "memchr/Cargo.toml",
        "[package]\nname='memchr'\nversion='0.1.0'\nedition='2024'\n[lib]\npath='../memchr-src/src/lib.rs'\n",
    );
    write(
        &root,
        "syntax/Cargo.toml",
        "[package]\nname='syntax-complete'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmemchr={path='../memchr'}\nunicode-ident={path='../unicode-ident-src'}\n",
    );
    write(
        &root,
        "Cargo.toml",
        "[package]\nname='typed-ast-readiness'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nsyntax-complete={path='syntax'}\n",
    );
    write(
        &root,
        "src/main.rs",
        include_str!("fixtures/typed_ast_readiness/basic.rs.txt"),
    );
    let generated =
        std::fs::read_to_string(repository.join("crates/flux-syntax/src/ast/generated.rs"))
            .unwrap();
    write(
        &root,
        "src/comprehensive.rs",
        &support::comprehensive_consumer(&generated),
    );
    write(
        &root,
        "source-inventory.txt",
        &inventory
            .iter()
            .map(|(a, b)| format!("{} -> {}\n", a.display(), b.display()))
            .collect::<String>(),
    );
    assert_identical(&inventory);
    let memchr = reference_library(&root, "memchr", "memchr-src/src/lib.rs", "2024", &[]);
    let unicode = reference_library(
        &root,
        "unicode_ident",
        "unicode-ident-src/src/lib.rs",
        "2021",
        &[],
    );
    let syntax = reference_library(
        &root,
        "syntax_complete",
        "syntax/src/lib.rs",
        "2024",
        &[("memchr", memchr), ("unicode_ident", unicode)],
    );
    for consumer in ["src/main.rs", "src/comprehensive.rs"] {
        let mut command = Command::new("rustc");
        command
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(root.join(consumer))
            .arg("--extern")
            .arg(format!("syntax_complete={}", syntax.display()))
            .arg("-L")
            .arg(&root);
        run(&root, &consumer.replace('/', "-"), &mut command);
        analyze(&root, consumer);
    }
    assert_identical(&inventory);
    eprintln!(
        "full typed AST readiness including independent rustc oracle: {:?}",
        started.elapsed()
    );
}
