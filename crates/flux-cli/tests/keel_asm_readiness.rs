//! Native differential of the assembler at the workspace Keel pin.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use serde_json::Value;
fn success(output: &Output, root: &Path, label: &str) {
    assert!(
        output.status.success(),
        "{label}: {}; see {}",
        String::from_utf8_lossy(&output.stderr),
        root.display()
    );
}

fn bounded(command: &mut Command, root: &Path, label: &str, seconds: u64) -> Output {
    support::process::run(command, root, label, Duration::from_secs(seconds)).output
}

fn phase(root: &Path, name: &str) {
    std::fs::write(root.join("phase.txt"), format!("{name}\n")).unwrap();
}

use support::inventory::collect as inventory;

#[test]
fn unchanged_keel_asm_matches_rustc() {
    if std::env::var_os("FLUX_KEEL_ASM_ROOT").is_some() {
        keel_asm_worker();
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/keel-asm-readiness/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut worker = Command::new(std::env::current_exe().unwrap());
    worker
        .args(["--exact", "unchanged_keel_asm_matches_rustc", "--nocapture"])
        .env("FLUX_KEEL_ASM_ROOT", &root);
    let output = bounded(&mut worker, &root, "audit", 300);
    // Also compare after a worker panic, abort, or timeout. A killed worker
    // cannot run its own final integrity check.
    let inventory_path = root.join("source-inventory.json");
    if inventory_path.exists() {
        let originals: Vec<(PathBuf, Vec<u8>)> =
            serde_json::from_slice(&std::fs::read(&inventory_path).unwrap()).unwrap();
        let changed: Vec<_> = support::inventory::changed(&originals);
        std::fs::write(
            root.join("parent-source-integrity.json"),
            serde_json::to_vec_pretty(&changed).unwrap(),
        )
        .unwrap();
        assert!(
            changed.is_empty(),
            "original source bytes changed during audit: {changed:?}"
        );
    }
    success(&output, &root, "audit");
    assert!(
        inventory_path.is_file(),
        "audit worker did not produce a source inventory; see {}",
        root.display()
    );
}

fn keel_asm_worker() {
    let root = PathBuf::from(std::env::var_os("FLUX_KEEL_ASM_ROOT").expect("run the parent test"));
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let workspace = metadata(&repo.join("Cargo.toml"), &root, "workspace-metadata", true);
    let workspace_packages = workspace["packages"].as_array().unwrap();
    let assembler = workspace_packages
        .iter()
        .find(|package| package["name"] == "keel-asm")
        .unwrap();
    let source = assembler["source"].as_str().unwrap();
    let (rev, resolved) = source
        .strip_prefix("git+https://github.com/hxyulin/keel?rev=")
        .unwrap()
        .split_once('#')
        .unwrap();
    assert_eq!(rev, resolved, "Keel must be pinned to a commit");

    let manifest = root.join("Cargo.toml");
    let lockfile = root.join("Cargo.lock");
    let fixture = repo.join("crates/flux-cli/tests/fixtures/keel_asm/consumer.rs.txt");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::copy(&fixture, root.join("src/main.rs")).unwrap();
    std::fs::copy(repo.join("Cargo.lock"), &lockfile).unwrap();
    std::fs::write(
        &manifest,
        format!(
            r#"[workspace]
[package]
name = "keel-asm-consumer"
version = "0.0.0"
edition = "2024"

[dependencies]
keel-asm = {{ git = "https://github.com/hxyulin/keel", rev = "{rev}" }}
object = {{ version = "=0.36.7", default-features = false, features = ["read", "std", "write"] }}
"#
        ),
    )
    .unwrap();
    // Prune the copied workspace lockfile to the consumer's closure. Verify
    // that every resolved dependency still has its workspace package ID.
    let graph = metadata(&manifest, &root, "consumer-metadata", false);
    let mut originals = Vec::new();
    for package in graph["packages"].as_array().unwrap() {
        if package["source"].is_null() {
            assert_eq!(package["name"], "keel-asm-consumer");
            continue;
        }
        assert!(
            workspace_packages.iter().any(|p| p["id"] == package["id"]),
            "package outside the workspace lock: {}",
            package["id"]
        );
        let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap());
        inventory(manifest.parent().unwrap(), &mut originals);
        originals.push((manifest.clone(), std::fs::read(&manifest).unwrap()));
        if package["source"] == source {
            let workspace = manifest.ancestors().nth(3).unwrap().join("Cargo.toml");
            originals.push((workspace.clone(), std::fs::read(workspace).unwrap()));
        }
    }
    inventory(&repo.join("library"), &mut originals);
    for path in [
        repo.join("Cargo.toml"),
        repo.join("Cargo.lock"),
        repo.join("crates/flux-hir/src/source.rs"),
        repo.join("crates/flux-cli/tests/keel_asm_readiness.rs"),
        fixture,
        manifest.clone(),
        lockfile,
        root.join("src/main.rs"),
    ] {
        originals.push((path.clone(), std::fs::read(&path).unwrap()));
    }
    originals.sort_by(|a, b| a.0.cmp(&b.0));
    originals.dedup_by(|a, b| a.0 == b.0);
    std::fs::write(
        root.join("source-inventory.json"),
        serde_json::to_vec(&originals).unwrap(),
    )
    .unwrap();

    let result = std::panic::catch_unwind(|| {
        phase(&root, "rustc-reference");
        let reference = bounded(
            Command::new("cargo")
                .args(["build", "--offline", "--locked", "--manifest-path"])
                .arg(&manifest),
            &root,
            "rustc-build",
            120,
        );
        success(&reference, &root, "rustc-build");
        let expected = bounded(
            &mut Command::new(root.join("target/debug/keel-asm-consumer")),
            &root,
            "rustc-run",
            30,
        );
        success(&expected, &root, "rustc-run");
        for mode in ["debug", "release"] {
            integrity(&root, &originals, &format!("pre-{mode}-source-integrity"));
            phase(&root, &format!("flux-{mode}"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command
                .args([
                    "build",
                    "--offline",
                    "--macro-host-compiler",
                    "rustc",
                    "-j1",
                ])
                .arg("--manifest-path")
                .arg(&manifest);
            if mode == "release" {
                command.arg("--release");
            }
            let output = bounded(&mut command, &root, &format!("flux-build-{mode}"), 180);
            success(&output, &root, &format!("flux-build-{mode}"));
            let actual = bounded(
                &mut Command::new(root.join(format!("target/flux/{mode}/keel-asm-consumer"))),
                &root,
                &format!("flux-run-{mode}"),
                30,
            );
            assert_eq!(actual.status.code(), expected.status.code(), "{mode}");
            assert_eq!(actual.stdout, expected.stdout, "{mode}");
            assert_eq!(actual.stderr, expected.stderr, "{mode}");
        }
        phase(&root, "native-verified");
    });
    integrity(&root, &originals, "source-integrity");
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

fn metadata(manifest: &Path, root: &Path, label: &str, locked: bool) -> Value {
    let mut command = Command::new("cargo");
    command
        .args([
            "metadata",
            "--offline",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(manifest);
    if locked {
        command.arg("--locked");
    }
    let output = bounded(&mut command, root, label, 60);
    success(&output, root, label);
    serde_json::from_slice(&output.stdout).unwrap()
}

fn integrity(root: &Path, originals: &[(PathBuf, Vec<u8>)], label: &str) {
    let changed: Vec<_> = support::inventory::changed(originals);
    std::fs::write(
        root.join(format!("{label}.json")),
        serde_json::to_vec_pretty(&changed).unwrap(),
    )
    .unwrap();
    assert!(
        changed.is_empty(),
        "original source bytes changed during audit: {changed:?}"
    );
}
