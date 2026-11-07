//! Registry manifests and exported macros compile through the Cargo source driver.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    let output = support::process::run(command, root, label, Duration::from_secs(90)).completed();
    support::oracle::success(&output, label);
    output
}

#[test]
fn registry_bitflags_matches_rustc() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/registry-bitflags"),
    );
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        r#"
[workspace]
[package]
name = "registry-bitflags-consumer"
version = "0.1.0"
edition = "2024"
[dependencies]
bitflags = { version = "=2.13.2", features = ["std"] }
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        include_str!("fixtures/bitflags/consumer.rs.txt"),
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
        .find(|p| p["name"] == "bitflags")
        .unwrap();
    assert_eq!(package["version"], "2.13.2");
    assert!(package["source"].as_str().unwrap().starts_with("registry+"));
    let upstream = Path::new(package["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap();
    let mut originals = Vec::new();
    support::inventory::collect(&upstream.join("src"), &mut originals);
    originals.push((
        upstream.join("Cargo.toml"),
        std::fs::read(upstream.join("Cargo.toml")).unwrap(),
    ));
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
            &mut Command::new(root.join("target/debug/registry-bitflags-consumer")),
            root,
            "rustc-run",
        );
        assert!(expected.stdout.ends_with(b"registry bitflags complete\n"));
        for optimized in [false, true] {
            let plan = root.join(format!("plan-{optimized}.json"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command
                .args(["build", "--cargo", "--offline", "--manifest-path"])
                .arg(&manifest)
                .arg("--plan")
                .arg(&plan);
            if optimized {
                command.arg("--release");
            }
            run(&mut command, root, &format!("flux-build-{optimized}"));
            let plan: serde_json::Value =
                serde_json::from_slice(&std::fs::read(plan).unwrap()).unwrap();
            assert_eq!(plan["blockers"], serde_json::json!([]));
            let source = plan["sources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["name"] == "bitflags")
                .unwrap();
            assert_eq!(source["features"], serde_json::json!(["std"]));
            let actual = run(
                &mut Command::new(root.join(if optimized {
                    "target/flux/release/registry-bitflags-consumer"
                } else {
                    "target/flux/debug/registry-bitflags-consumer"
                })),
                root,
                &format!("flux-run-{optimized}"),
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
