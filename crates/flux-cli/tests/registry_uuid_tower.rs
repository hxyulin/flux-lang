//! UUID parsing and Tower layers compile from unchanged registry sources.
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
fn registry_uuid_and_tower_match_rustc() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/registry-uuid-tower"),
    );
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        r#"
[workspace]
[package]
name = "registry-uuid-tower-consumer"
version = "0.1.0"
edition = "2024"
[dependencies]
uuid = "=1.29.0"
tower-layer = "=0.3.3"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        include_str!("fixtures/uuid-tower/consumer.rs.txt"),
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
    let mut originals = Vec::new();
    for (name, version) in [("uuid", "1.29.0"), ("tower-layer", "0.3.3")] {
        let package = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap();
        assert_eq!(package["version"], version);
        assert!(package["source"].as_str().unwrap().starts_with("registry+"));
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
            &mut Command::new(root.join("target/debug/registry-uuid-tower-consumer")),
            root,
            "rustc-run",
        );
        assert!(
            expected
                .stdout
                .ends_with(b"uuid and tower-layer complete\n")
        );
        for optimized in [false, true] {
            let plan = root.join(format!("plan-{optimized}.json"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command
                .args(["cargo", "build", "--offline", "--manifest-path"])
                .arg(&manifest)
                .arg("--plan")
                .arg(&plan);
            if optimized {
                command.arg("-O");
            }
            run(&mut command, root, &format!("flux-build-{optimized}"));
            let plan: serde_json::Value =
                serde_json::from_slice(&std::fs::read(plan).unwrap()).unwrap();
            assert_eq!(plan["blockers"], serde_json::json!([]));
            for name in ["uuid", "tower_layer"] {
                assert!(
                    plan["sources"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|s| s["name"] == name)
                );
            }
            let actual = run(
                &mut Command::new(root.join("target/flux/registry-uuid-tower-consumer")),
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
