//! Unchanged default-feature registry graphs, with native rustc comparisons.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    let output = support::process::run(command, root, label, Duration::from_secs(600)).completed();
    support::oracle::success(&output, label);
    output
}
fn compare_registry(name: &str, version: &str, source: &str) {
    compare_registry_dependencies(name, version, source, &[]);
}
fn compare_registry_dependencies(name: &str, version: &str, source: &str, extras: &[(&str, &str)]) {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../out/corpus-crates/{name}")),
    );
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    let extra_dependencies: String = extras
        .iter()
        .map(|(name, version)| format!("{name}=\"={version}\"\n"))
        .collect();
    std::fs::write(root.join("Cargo.toml"), format!(
        "[workspace]\n[package]\nname=\"corpus-crate-consumer\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\n{name}=\"={version}\"\n{extra_dependencies}"
    )).unwrap();
    std::fs::write(root.join("src/main.rs"), source).unwrap();
    let manifest = root.join("Cargo.toml");
    run(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(&manifest),
        root,
        "lock",
    );
    let metadata = run(
        Command::new("cargo")
            .args([
                "metadata",
                "--offline",
                "--locked",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(&manifest),
        root,
        "metadata",
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let mut originals = Vec::new();
    for package in metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["source"].is_string())
    {
        let upstream = Path::new(package["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap();
        support::inventory::collect(&upstream.join("src"), &mut originals);
        originals.push((
            upstream.join("Cargo.toml"),
            std::fs::read(upstream.join("Cargo.toml")).unwrap(),
        ));
    }
    let lock = std::fs::read(root.join("Cargo.lock")).unwrap();
    let outcome = std::panic::catch_unwind(|| {
        run(
            Command::new("cargo")
                .args(["build", "--offline", "--locked", "--manifest-path"])
                .arg(&manifest),
            root,
            "rustc-build",
        );
        let expected = run(
            &mut Command::new(root.join("target/debug/corpus-crate-consumer")),
            root,
            "rustc-run",
        );
        for mode in ["debug", "fast", "release"] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command
                .args([
                    "build",
                    "--offline",
                    "--macro-host-compiler",
                    "rustc",
                    "-j1",
                    "--manifest-path",
                ])
                .arg(&manifest);
            if mode == "fast" {
                command.arg("--fast-build");
            }
            if mode == "release" {
                command.arg("--release");
            }
            run(&mut command, root, &format!("flux-build-{mode}"));
            let actual = run(
                &mut Command::new(root.join(format!("target/flux/{mode}/corpus-crate-consumer"))),
                root,
                &format!("flux-run-{mode}"),
            );
            assert_eq!(
                actual.status.code(),
                expected.status.code(),
                "{name} {mode}"
            );
            assert_eq!(actual.stdout, expected.stdout, "{name} {mode}");
            assert_eq!(actual.stderr, expected.stderr, "{name} {mode}");
        }
    });
    assert!(
        support::inventory::changed(&originals).is_empty(),
        "registry sources changed"
    );
    assert_eq!(std::fs::read(root.join("Cargo.lock")).unwrap(), lock);
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
}
#[test]
fn itertools_default_graph_matches_rustc() {
    compare_registry(
        "itertools",
        "0.15.0",
        include_str!("fixtures/corpus_crates/itertools.rs.txt"),
    );
}

#[test]
fn proc_macro2_default_graph_matches_rustc() {
    compare_registry(
        "proc-macro2",
        "1.0.107",
        include_str!("fixtures/corpus_crates/proc_macro2.rs.txt"),
    );
}

#[test]
fn quote_default_graph_matches_rustc() {
    compare_registry(
        "quote",
        "1.0.47",
        include_str!("fixtures/corpus_crates/quote.rs.txt"),
    );
}

#[test]
fn syn_default_graph_matches_rustc() {
    compare_registry(
        "syn",
        "3.0.10",
        include_str!("fixtures/corpus_crates/syn.rs.txt"),
    );
}

#[test]
fn url_default_graph_matches_rustc() {
    compare_registry(
        "url",
        "2.5.8",
        include_str!("fixtures/corpus_crates/url.rs.txt"),
    );
}

#[test]
fn tracing_subscriber_default_graph_matches_rustc() {
    compare_registry_dependencies(
        "tracing-subscriber",
        "0.3.23",
        include_str!("fixtures/corpus_crates/tracing_subscriber.rs.txt"),
        &[("tracing", "0.1.44")],
    );
}
