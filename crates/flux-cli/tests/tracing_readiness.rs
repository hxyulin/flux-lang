//! Tracing spans, events, projected dispatch guards, and wide callsite hashes.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    let output = support::process::run(command, root, label, Duration::from_secs(180)).completed();
    support::oracle::success(&output, label);
    output
}

#[test]
fn registry_tracing_matches_rustc() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/tracing-readiness"),
    );
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[workspace]\n[package]\nname=\"tracing-readiness\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\ntracing=\"=0.1.44\"\n").unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        include_str!("fixtures/tracing/consumer.rs.txt"),
    )
    .unwrap();
    let manifest = root.join("Cargo.toml");
    run(
        Command::new("cargo")
            .args(["build", "--offline", "--manifest-path"])
            .arg(&manifest),
        root,
        "rustc-build",
    );
    let expected = run(
        &mut Command::new(root.join("target/debug/tracing-readiness")),
        root,
        "rustc-run",
    );
    assert_eq!(expected.stdout, b"1 2 2\n");
    let lock = std::fs::read(root.join("Cargo.lock")).unwrap();
    for mode in ["debug", "fast", "release"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .args([
                "build",
                "--offline",
                "--macro-host-compiler",
                "rustc",
                "--manifest-path",
            ])
            .arg(&manifest)
            .args(["-j1"]);
        if mode == "fast" {
            command.arg("--fast-build");
        }
        if mode == "release" {
            command.arg("--release");
        }
        run(&mut command, root, &format!("flux-build-{mode}"));
        let profile = if mode == "fast" { "fast" } else { mode };
        let actual = run(
            &mut Command::new(root.join(format!("target/flux/{profile}/tracing-readiness"))),
            root,
            &format!("flux-run-{mode}"),
        );
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
        assert_eq!(std::fs::read(root.join("Cargo.lock")).unwrap(), lock);
    }
}
