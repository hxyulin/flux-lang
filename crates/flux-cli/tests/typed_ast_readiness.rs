//! Complete unchanged syntax source graph, including the typed AST, compiled
//! and executed by Flux and rustc. Dependency sources use the bounded memchr
//! edition-2024 wrapper and original unicode-ident edition-2021 manifest.
//! This source-graph test does not build the workspace or bootstrap Flux.
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
    let (db, _, hir) = flux_hir::lower_text(root.join("negative.rs"), source);
    assert!(db.syntax_errors().is_empty());
    assert!(hir.diagnostics.is_empty());
    let typed = flux_mir::check_file(&hir);
    record(&root, "type", &db, &typed.diagnostics);
    assert_eq!(typed.diagnostics.len(), 1, "{:?}", typed.diagnostics);
    assert!(typed.diagnostics[0].message.contains("mismatched types"));
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

fn analyze(root: &Path, consumer: &str, expected: &std::process::Output) {
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
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    assert!(flux_ty::check_main(&hir.items, &typed.sigs).is_none());
    let mir = flux_mir::build_file(&hir, &typed);
    record(root, &format!("{consumer}.mir"), &db, &mir.diagnostics);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    for (label, tier) in [
        ("debug", flux_codegen::OptimizationTier::None),
        ("optimized", flux_codegen::OptimizationTier::Full),
    ] {
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        let target = flux_codegen::Target::Aarch64AppleDarwin;
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
        let executable = root.join(format!("{}-{label}", consumer.replace('/', "-")));
        flux_codegen::link_executable(&object, target, &executable).unwrap();
        let actual = Command::new(&executable).output().unwrap();
        let prefix = format!("{}-{label}", consumer.replace('/', "-"));
        write(
            root,
            &format!("{prefix}.stdout"),
            std::str::from_utf8(&actual.stdout).unwrap(),
        );
        write(
            root,
            &format!("{prefix}.stderr"),
            std::str::from_utf8(&actual.stderr).unwrap(),
        );
        assert_eq!(
            actual.status.code(),
            expected.status.code(),
            "{consumer} {label}"
        );
        assert_eq!(actual.stdout, expected.stdout, "{consumer} {label}");
        assert_eq!(actual.stderr, expected.stderr, "{consumer} {label}");
    }
    eprintln!(
        "{consumer}: clean syntax/HIR/types/MIR; native modes match rustc ({:?})",
        started.elapsed()
    );
}

#[test]
fn full_unchanged_typed_ast_source_graph_matches_rustc() {
    let started = Instant::now();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Preserve the complete reproducer and all diagnostics, including on failure.
    let root =
        std::env::temp_dir().join(format!("flux-typed-ast-readiness-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    eprintln!("typed AST native comparison reproducer: {}", root.display());
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
            .args([
                "--edition=2024",
                "-C",
                "panic=abort",
                "-C",
                "overflow-checks=off",
                "-o",
            ])
            .arg(root.join("reference"))
            .arg(root.join(consumer))
            .arg("--extern")
            .arg(format!("syntax_complete={}", syntax.display()))
            .arg("-L")
            .arg(&root);
        run(&root, &consumer.replace('/', "-"), &mut command);
        let expected = Command::new(root.join("reference")).output().unwrap();
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        assert!(!expected.stdout.is_empty());
        analyze(&root, consumer, &expected);
    }
    assert_identical(&inventory);
    eprintln!(
        "full typed AST native comparison including independent rustc oracle: {:?}",
        started.elapsed()
    );
}
