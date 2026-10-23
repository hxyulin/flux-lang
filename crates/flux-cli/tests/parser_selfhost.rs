//! A restricted wrapper around unchanged production lexer and parser modules.
//! The wrapper omits typed AST/tree construction; Edition and TextRange are
//! copied verbatim from their production definitions. No lexer/parser method
//! is removed, including Input::from_lexed. Dependency sources are unchanged:
//! memchr uses an explicit feature-free edition-2024 manifest, unicode-ident
//! retains its original manifest. This is not a full flux-syntax crate build.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn unchanged_parser_sources_match_rustc() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Keep the assembled production-source reproducer, including on failure.
    let root = std::env::temp_dir().join(format!("flux-parser-selfhost-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
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
    success(&metadata);
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    for name in ["memchr", "unicode-ident"] {
        let package = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap();
        let manifest = Path::new(package["manifest_path"].as_str().unwrap());
        copy_tree(
            manifest.parent().unwrap(),
            &root.join(format!("{name}-src")),
        );
    }
    write(
        &root,
        "memchr/Cargo.toml",
        "[package]\nname='memchr'\nversion='0.1.0'\nedition='2024'\n[lib]\npath='../memchr-src/src/lib.rs'\n",
    );
    write(
        &root,
        "Cargo.toml",
        "[package]\nname='parser-selfhost'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nparser-subset={path='parser-subset'}\n",
    );
    write(
        &root,
        "parser-subset/Cargo.toml",
        "[package]\nname='parser-subset'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmemchr={path='../memchr'}\nunicode-ident={path='../unicode-ident-src'}\n",
    );
    let syntax = repository.join("crates/flux-syntax/src");
    copy_tree(
        &syntax.join("parser"),
        &root.join("parser-subset/src/parser"),
    );
    for name in ["lexer.rs", "syntax_kind.rs"] {
        std::fs::copy(syntax.join(name), root.join("parser-subset/src").join(name)).unwrap();
    }
    let lib = std::fs::read_to_string(syntax.join("lib.rs")).unwrap();
    let edition = lib
        .split("/// Rust edition")
        .nth(1)
        .unwrap()
        .split("/// A problem found")
        .next()
        .unwrap();
    let tree = std::fs::read_to_string(syntax.join("tree.rs")).unwrap();
    let range = tree
        .split("/// A byte range")
        .nth(1)
        .unwrap()
        .split("impl std::ops::Index")
        .next()
        .unwrap();
    write(
        &root,
        "parser-subset/src/lib.rs",
        &format!(
            "use std::fmt;\npub mod parser;\npub mod lexer;\nmod syntax_kind;\npub use syntax_kind::SyntaxKind;\npub use lexer::LexedStr;\n/// Rust edition{edition}\n/// A byte range{range}"
        ),
    );
    write(
        &root,
        "src/main.rs",
        include_str!("fixtures/parser/consumer.rs.txt"),
    );
    let mut corpus = String::from("pub const FILES: &[(&str, &str, bool)] = &[\n");
    for (directory, errors) in [("ok", false), ("err", true)] {
        let mut paths: Vec<_> = std::fs::read_dir(
            repository
                .join("crates/flux-syntax/tests/data/parser")
                .join(directory),
        )
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
        paths.sort();
        for path in paths {
            let name = format!(
                "{directory}/{}",
                path.file_name().unwrap().to_str().unwrap()
            );
            let text = std::fs::read_to_string(path).unwrap();
            corpus.push_str(&format!("({name:?}, {text:?}, {errors}),\n"));
        }
    }
    corpus.push_str("];\n");
    write(&root, "src/corpus.rs", &corpus);
    write(
        &root,
        "deny-rustc",
        "#!/bin/sh\nprintf 'rustc invoked\\n' >> \"$RUSTC_CALL_LOG\"\nexit 86\n",
    );
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        root.join("deny-rustc"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    eprintln!("parser self-host reproducer: {}", root.display());
    let mut first_mode = None;
    for optimized in [false, true] {
        let label = if optimized {
            "optimized"
        } else {
            "unoptimized"
        };
        let mode: &[&str] = if optimized { &["-O"] } else { &[] };
        let memchr = reference_library(&root, "memchr", "memchr-src/src/lib.rs", "2024", mode, &[]);
        let unicode = reference_library(
            &root,
            "unicode_ident",
            "unicode-ident-src/src/lib.rs",
            "2021",
            mode,
            &[],
        );
        let parser = reference_library(
            &root,
            "parser_subset",
            "parser-subset/src/lib.rs",
            "2024",
            mode,
            &[("memchr", memchr), ("unicode_ident", unicode)],
        );
        let reference = root.join("reference");
        let output = Command::new("rustc")
            .args([
                "--edition=2024",
                "-C",
                "panic=abort",
                "-C",
                "overflow-checks=off",
            ])
            .args(mode)
            .arg(root.join("src/main.rs"))
            .arg("--extern")
            .arg(format!("parser_subset={}", parser.display()))
            .arg("-L")
            .arg(&root)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        success(&output);
        let expected = Command::new(&reference).output().unwrap();
        success(&expected);
        std::fs::write(
            root.join(format!("reference-{label}.stdout")),
            &expected.stdout,
        )
        .unwrap();
        if let Some(first) = &first_mode {
            assert_eq!(first, &expected.stdout);
        } else {
            first_mode = Some(expected.stdout.clone());
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(&root)
            .args(["package", "run", "--manifest-path", "Cargo.toml"])
            .args(mode);
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.contains("RUSTFLAGS")
                || name.contains("RUSTDOCFLAGS")
                || name.starts_with("CARGO_BUILD_")
                || name.starts_with("CARGO_TARGET_")
                || name.starts_with("CARGO_PROFILE_")
                || name.starts_with("CARGO_UNSTABLE_")
                || name == "CARGO_INCREMENTAL"
            {
                command.env_remove(key);
            }
        }
        command
            .env("CARGO_HOME", root.join("cargo-home"))
            .env("RUSTC", root.join("deny-rustc"))
            .env("RUSTC_WRAPPER", root.join("deny-rustc"))
            .env("RUSTC_WORKSPACE_WRAPPER", root.join("deny-rustc"))
            .env("RUSTC_CALL_LOG", root.join("rustc-calls"));
        let actual = command.output().unwrap();
        assert!(!root.join("rustc-calls").exists(), "Flux invoked rustc");
        std::fs::write(root.join(format!("flux-{label}.stdout")), &actual.stdout).unwrap();
        std::fs::write(root.join(format!("flux-{label}.stderr")), &actual.stderr).unwrap();
        success(&actual);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}

fn reference_library(
    root: &Path,
    name: &str,
    source: &str,
    edition: &str,
    mode: &[&str],
    dependencies: &[(&str, PathBuf)],
) -> PathBuf {
    let library = root.join(format!("lib{name}.rlib"));
    let mut command = Command::new("rustc");
    command
        .args([
            "-C",
            "panic=abort",
            "-C",
            "overflow-checks=off",
            "--crate-type=rlib",
            "--crate-name",
            name,
            "--edition",
            edition,
        ])
        .args(mode)
        .arg(root.join(source))
        .arg("-L")
        .arg(root)
        .arg("-o")
        .arg(&library);
    for (name, path) in dependencies {
        command
            .arg("--extern")
            .arg(format!("{name}={}", path.display()));
    }
    success(&command.output().unwrap());
    library
}
