//! Executable lexer -> parser -> lossless tree milestone, not a whole-crate build.
//! All production build/tree/lexer/kind/parser sources remain byte-for-byte intact.
//! The lib wrapper retains production declarations and parse entry points, excluding
//! typed AST and its one accessor until trait/body lifetime support is available.
//! Flux consumes the source graph directly; only the rustc reference uses rlibs.
//! memchr 2.8.3 uses an explicit feature-free edition-2024 manifest;
//! unicode-ident 1.0.27 keeps its original edition-2021 manifest.
//! Offline Cargo metadata locates cached sources without compiling dependencies.
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
fn unchanged_tree_pipeline_sources_match_rustc() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Keep the assembled production-source reproducer, including on failure.
    let root = std::env::temp_dir().join(format!(
        "flux-tree-pipeline-selfhost-{}",
        std::process::id()
    ));
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
        "[package]\nname='tree-pipeline-selfhost'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ntree-subset={path='tree-subset'}\n",
    );
    write(
        &root,
        "tree-subset/Cargo.toml",
        "[package]\nname='tree-subset'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmemchr={path='../memchr'}\nunicode-ident={path='../unicode-ident-src'}\n",
    );
    let syntax = repository.join("crates/flux-syntax/src");
    copy_tree(&syntax.join("parser"), &root.join("tree-subset/src/parser"));
    for name in ["lexer.rs", "syntax_kind.rs", "build.rs", "tree.rs"] {
        std::fs::copy(syntax.join(name), root.join("tree-subset/src").join(name)).unwrap();
    }
    let lib = std::fs::read_to_string(syntax.join("lib.rs")).unwrap();
    let wrapper = lib.split("#[cfg(test)]").next().unwrap();
    let accessor = wrapper
        .split("    /// The root as a typed node.")
        .nth(1)
        .unwrap()
        .split("    pub fn errors")
        .next()
        .unwrap();
    let wrapper = wrapper
        .replace("pub mod ast;\n", "")
        .replace(&format!("    /// The root as a typed node.{accessor}"), "");
    assert!(!wrapper.contains("ast::"));
    write(&root, "tree-subset/src/lib.rs", &wrapper);
    write(
        &root,
        "src/main.rs",
        include_str!("fixtures/tree_pipeline/consumer.rs.txt"),
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
    corpus.push_str("pub const WORKSPACE: &[(&str, &str)] = &[\n");
    for path in [
        "crates/flux-syntax/src/build.rs",
        "crates/flux-syntax/src/tree.rs",
        "crates/flux-syntax/src/lexer.rs",
        "crates/flux-syntax/src/parser/mod.rs",
        "crates/flux-hir/src/lib.rs",
        "library/core/src/lib.rs",
    ] {
        let text = std::fs::read_to_string(repository.join(path)).unwrap();
        corpus.push_str(&format!("({path:?}, {text:?}),\n"));
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
    eprintln!("tree pipeline self-host reproducer: {}", root.display());
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
        let tree = reference_library(
            &root,
            "tree_subset",
            "tree-subset/src/lib.rs",
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
            .arg(format!("tree_subset={}", tree.display()))
            .arg("-L")
            .arg(&root)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        success(&output);
        let expected = Command::new(&reference).output().unwrap();
        std::fs::write(
            root.join(format!("reference-{label}.stdout")),
            &expected.stdout,
        )
        .unwrap();
        std::fs::write(
            root.join(format!("reference-{label}.stderr")),
            &expected.stderr,
        )
        .unwrap();
        success(&expected);
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
        assert!(
            actual.stdout == expected.stdout,
            "{label} stdout differs; full outputs and source graph: {}",
            root.display()
        );
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
