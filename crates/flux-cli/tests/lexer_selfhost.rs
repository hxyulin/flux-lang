//! Compile the unchanged production lexer through an explicit AST-free boundary.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Probe(PathBuf);

impl Probe {
    fn write(&self, path: &str, contents: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        // Preserve the complete source graph and executable on failure.
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn copy(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &destination.join(entry.file_name()));
        } else {
            std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn production_lexer_matches_rustc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let metadata = success(
        Command::new("cargo")
            .current_dir(&root)
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
            .unwrap(),
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let probe =
        Probe(std::env::temp_dir().join(format!("flux-lexer-selfhost-{}", std::process::id())));
    eprintln!("source graph: {}", probe.0.display());
    std::fs::create_dir_all(&probe.0).unwrap();
    for name in ["memchr", "unicode-ident"] {
        let package = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap();
        let manifest = Path::new(package["manifest_path"].as_str().unwrap());
        eprintln!("{} {}: {}", name, package["version"], manifest.display());
        copy(
            manifest.parent().unwrap(),
            &probe.0.join(format!("{name}-src")),
        );
    }
    // memchr's original manifest is outside the package driver's supported
    // feature-free subset. Keep all source files unchanged and name that boundary.
    probe.write("memchr/Cargo.toml", "[package]\nname='memchr'\nversion='0.0.0'\nedition='2024'\n[lib]\npath='../memchr-src/src/lib.rs'\n");
    probe.write("Cargo.toml", "[package]\nname='lexer-probe'\nversion='0.0.0'\nedition='2024'\n[dependencies]\nsyntax-lexer={path='syntax'}\n");
    probe.write("syntax/Cargo.toml", "[package]\nname='syntax-lexer'\nversion='0.0.0'\nedition='2024'\n[dependencies]\nmemchr={path='../memchr'}\nunicode-ident={path='../unicode-ident-src'}\n");
    for name in ["lexer.rs", "syntax_kind.rs", "tree.rs"] {
        probe.write(
            &format!("syntax/src/{name}"),
            &std::fs::read_to_string(root.join("crates/flux-syntax/src").join(name)).unwrap(),
        );
    }
    let original = std::fs::read_to_string(root.join("crates/flux-syntax/src/lib.rs")).unwrap();
    let edition = original
        .split("/// Rust edition of one source crate.")
        .nth(1)
        .unwrap()
        .split("/// A problem found")
        .next()
        .unwrap();
    let edition = format!("/// Rust edition of one source crate.{edition}");
    probe.write("syntax/src/lib.rs", &format!("mod syntax_kind; mod tree; mod lexer;\npub use syntax_kind::SyntaxKind; pub use tree::TextRange;\n{edition}\nmod comparison; pub use comparison::run;\n"));
    probe.write(
        "syntax/src/comparison.rs",
        include_str!("support/lexer_comparison.rs"),
    );
    let corpus_paths = [
        "crates/flux-syntax/src/lexer.rs",
        "crates/flux-syntax/src/syntax_kind.rs",
        "crates/flux-syntax/src/tree.rs",
        "crates/flux-hir/src/macros/tt.rs",
        "crates/flux-ty/src/ty.rs",
        "crates/flux-mir/src/bitset.rs",
        "crates/flux-difftest/src/rng.rs",
        "library/core/src/str.rs",
    ];
    let mut corpus = String::from("pub const CORPUS: &[&str] = &[\n");
    for path in corpus_paths {
        let source = std::fs::read_to_string(root.join(path)).unwrap();
        corpus.push_str(&format!("{source:?},\n"));
    }
    corpus.push_str("];\n");
    probe.write("syntax/src/comparison/corpus.rs", &corpus);
    probe.write("src/main.rs", "fn main() { syntax_lexer::run(); }\n");
    probe.write(
        "deny-rustc",
        "#!/bin/sh\nprintf 'rustc invoked\\n' >> \"$RUSTC_CALL_LOG\"\nexit 86\n",
    );
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        probe.0.join("deny-rustc"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    for flags in [&[][..], &["-O"][..]] {
        let mut externs = Vec::new();
        for (name, path, edition) in [
            ("memchr", "memchr-src/src/lib.rs", "2024"),
            ("unicode_ident", "unicode-ident-src/src/lib.rs", "2021"),
            ("syntax_lexer", "syntax/src/lib.rs", "2024"),
        ] {
            let library = probe.0.join(format!("lib{name}.rlib"));
            let mut command = Command::new("rustc");
            command
                .args([
                    "--crate-type=rlib",
                    "--crate-name",
                    name,
                    "--edition",
                    edition,
                ])
                .args(flags)
                .arg(probe.0.join(path))
                .arg("-L")
                .arg(&probe.0)
                .arg("-o")
                .arg(&library);
            for dependency in &externs {
                command.arg("--extern").arg(dependency);
            }
            success(command.output().unwrap());
            externs.push(format!("{name}={}", library.display()));
        }
        let executable = probe.0.join("reference");
        let mut command = Command::new("rustc");
        command
            .args([
                "--edition=2024",
                "-C",
                "panic=abort",
                "-C",
                "overflow-checks=off",
            ])
            .args(flags)
            .arg(probe.0.join("src/main.rs"))
            .arg("-L")
            .arg(&probe.0)
            .arg("-o")
            .arg(&executable);
        for dependency in &externs {
            command.arg("--extern").arg(dependency);
        }
        success(command.output().unwrap());
        let expected = success(Command::new(executable).output().unwrap());
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(&probe.0)
            .args(["package", "run", "--manifest-path", "Cargo.toml"])
            .args(flags)
            .env("CARGO_HOME", probe.0.join("cargo-home"))
            .env("RUSTC_CALL_LOG", probe.0.join("rustc-calls"));
        for name in ["RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"] {
            command.env(name, probe.0.join("deny-rustc"));
        }
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
        let actual = success(command.output().unwrap());
        assert!(!probe.0.join("rustc-calls").exists());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
        eprintln!(
            "{}",
            String::from_utf8_lossy(&actual.stdout)
                .lines()
                .last()
                .unwrap()
        );
    }
}
