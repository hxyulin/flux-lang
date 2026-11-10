//! Unchanged Rayon runs parallel collections, borrowed iterators and scoped jobs.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    let output = support::process::run(command, root, label, Duration::from_secs(300)).completed();
    support::oracle::success(&output, label);
    output
}

#[test]
fn registry_rayon_matches_rustc() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/rayon-readiness"),
    );
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        r#"
[workspace]
[package]
name = "rayon-readiness-consumer"
version = "0.1.0"
edition = "2024"
[dependencies]
rayon = "=1.12.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        include_str!("fixtures/rayon/consumer.rs.txt"),
    )
    .unwrap();
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
                "--all-features",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(&manifest),
        root,
        "metadata",
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "rayon")
        .unwrap();
    assert_eq!(package["version"], "1.12.0");
    assert!(package["source"].as_str().unwrap().starts_with("registry+"));
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
            &mut Command::new(root.join("target/debug/rayon-readiness-consumer")),
            root,
            "rustc-run",
        );
        assert!(expected.stdout.ends_with(b"rayon scoped workers 17 23\n"));
        for profile in ["debug", "fast", "release"] {
            let plan = root.join(format!("plan-{profile}.json"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command
                .args([
                    "build",
                    "--offline",
                    "--macro-host-compiler",
                    "flux",
                    "-j1",
                    "--manifest-path",
                ])
                .arg(&manifest)
                .arg("--plan")
                .arg(&plan);
            match profile {
                "fast" => {
                    command.arg("--fast-build");
                }
                "release" => {
                    command.arg("--release");
                }
                _ => {}
            }
            run(&mut command, root, &format!("flux-build-{profile}"));
            let plan: serde_json::Value =
                serde_json::from_slice(&std::fs::read(plan).unwrap()).unwrap();
            assert_eq!(plan["kind"], "flux-native-prepared");
            assert_eq!(plan["macro_host_compiler"], "flux");
            assert!(
                plan["plan"]["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|unit| unit["target"]["name"] == "rayon")
            );
            let actual = run(
                &mut Command::new(
                    root.join(format!("target/flux/{profile}/rayon-readiness-consumer")),
                ),
                root,
                &format!("flux-run-{profile}"),
            );
            assert_eq!(actual.status.code(), expected.status.code());
            assert_eq!(actual.stdout, expected.stdout);
            assert_eq!(actual.stderr, expected.stderr);
        }
    });
    assert!(
        support::inventory::changed(&originals).is_empty(),
        "registry source bytes changed"
    );
    assert_eq!(std::fs::read(root.join("Cargo.lock")).unwrap(), lock);
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
}
