//! Unchanged futures-util runs with default features and optional I/O, sinks and channels.
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
fn registry_futures_util_matches_rustc() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/futures-util-readiness"),
    );
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        r#"
[workspace]
[package]
name = "futures-util-readiness-consumer"
version = "0.1.0"
edition = "2024"
[dependencies]
corpus_subject = { package = "futures-util", version = "=0.3.34" }
[features]
extended = ["corpus_subject/io", "corpus_subject/sink", "corpus_subject/channel"]
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        include_str!("fixtures/futures-util/consumer.rs.txt"),
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
        .find(|p| p["name"] == "futures-util")
        .unwrap();
    assert_eq!(package["version"], "0.3.34");
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
        for extended in [false, true] {
            let mut command = Command::new("cargo");
            command
                .args(["build", "--offline", "--locked", "--manifest-path"])
                .arg(&manifest);
            if extended {
                command.args(["--features", "extended"]);
            }
            run(&mut command, root, &format!("rustc-build-{extended}"));
            let expected = run(
                &mut Command::new(root.join("target/debug/futures-util-readiness-consumer")),
                root,
                &format!("rustc-run-{extended}"),
            );
            assert!(
                expected
                    .stdout
                    .ends_with(b"queue [5, 8] delayed 37 polls 3\n")
            );
            for host in ["rustc", "flux"] {
                for optimized in [false, true] {
                    let label = format!("{extended}-{optimized}-{host}");
                    let plan = root.join(format!("plan-{label}.json"));
                    let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
                    command
                        .args([
                            "build",
                            "--offline",
                            "--macro-host-compiler",
                            host,
                            "-j1",
                            "--manifest-path",
                        ])
                        .arg(&manifest)
                        .arg("--plan")
                        .arg(&plan);
                    if extended {
                        command.args(["--features", "extended"]);
                    }
                    if optimized {
                        command.arg("--release");
                    }
                    run(&mut command, root, &format!("flux-build-{label}"));
                    let plan: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(plan).unwrap()).unwrap();
                    assert_eq!(plan["kind"], "flux-native-prepared");
                    assert_eq!(plan["macro_host_compiler"], host);
                    let source = plan["plan"]["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|s| s["target"]["name"] == "futures_util")
                        .unwrap();
                    let features = source["features"].as_array().unwrap();
                    assert!(features.iter().any(|f| f == "async-await-macro"));
                    for feature in ["io", "sink", "channel"] {
                        assert_eq!(features.iter().any(|f| f == feature), extended);
                    }
                    let actual = run(
                        &mut Command::new(root.join(if optimized {
                            "target/flux/release/futures-util-readiness-consumer"
                        } else {
                            "target/flux/debug/futures-util-readiness-consumer"
                        })),
                        root,
                        &format!("flux-run-{label}"),
                    );
                    assert_eq!(actual.status.code(), expected.status.code());
                    assert_eq!(actual.stdout, expected.stdout);
                    assert_eq!(actual.stderr, expected.stderr);
                }
            }
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
