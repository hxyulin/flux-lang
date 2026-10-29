//! The pinned backend IR crate, with its original source files and edition.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use flux_hir::SourceDb;
use flux_syntax::Edition;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use support::inventory::collect as inventory;

use support::inventory::report;

#[test]
fn unchanged_keel_ir_matches_rustc() {
    if std::env::var_os("FLUX_KEEL_IR_ROOT").is_some() {
        worker();
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/keel-ir-readiness/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "unchanged_keel_ir_matches_rustc", "--nocapture"])
        .env("FLUX_KEEL_IR_ROOT", &root)
        .stdout(Stdio::from(
            std::fs::File::create(root.join("audit.stdout")).unwrap(),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(root.join("audit.stderr")).unwrap(),
        ))
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed().as_secs() >= 300 {
            child.kill().unwrap();
            break child.wait().unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    if let Ok(bytes) = std::fs::read(root.join("source-inventory.json")) {
        let originals: Vec<(PathBuf, Vec<u8>)> = serde_json::from_slice(&bytes).unwrap();
        let changed: Vec<_> = originals
            .iter()
            .filter(|(p, bytes)| std::fs::read(p).ok().as_ref() != Some(bytes))
            .map(|(p, _)| p)
            .collect();
        std::fs::write(
            root.join("parent-source-integrity.json"),
            serde_json::to_vec(&changed).unwrap(),
        )
        .unwrap();
        assert!(changed.is_empty(), "audit inputs changed: {changed:?}");
    }
    assert!(status.success(), "audit failed; see {}", root.display());
}

fn worker() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = PathBuf::from(std::env::var_os("FLUX_KEEL_IR_ROOT").unwrap());
    let metadata = Command::new("cargo")
        .current_dir(&repo)
        .args(["metadata", "--offline", "--locked", "--format-version", "1"])
        .output()
        .unwrap();
    assert!(metadata.status.success());
    std::fs::write(root.join("metadata.json"), &metadata.stdout).unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "keel-ir")
        .unwrap();
    assert_eq!(package["edition"], "2024");
    let node = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == package["id"])
        .unwrap();
    assert!(node["features"].as_array().unwrap().is_empty());
    let mut originals = Vec::new();
    for dependency in support::keel_ir::packages(&metadata) {
        inventory(
            support::keel_ir::source(dependency).parent().unwrap(),
            &mut originals,
        );
        let manifest = PathBuf::from(dependency["manifest_path"].as_str().unwrap());
        originals.push((manifest.clone(), std::fs::read(manifest).unwrap()));
    }
    inventory(&repo.join("library"), &mut originals);
    for path in [
        repo.join("Cargo.lock"),
        repo.join("crates/flux-hir/src/source.rs"),
        PathBuf::from(package["manifest_path"].as_str().unwrap()),
    ] {
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }
    std::fs::write(
        root.join("source-inventory.json"),
        serde_json::to_vec(&originals).unwrap(),
    )
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let consumer = root.join("consumer.rs");
        std::fs::write(&consumer, include_str!("fixtures/keel_ir/consumer.rs.txt")).unwrap();
        let library = support::keel_ir::compile_reference(&metadata, &root);
        let reference = root.join("reference");
        let output = Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&consumer)
            .arg("--extern")
            .arg(format!("keel_ir={}", library.display()))
            .arg("-L")
            .arg(format!("dependency={}", root.display()))
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = Command::new(reference).output().unwrap();
        assert!(expected.status.success());
        std::fs::write(root.join("reference.stdout"), &expected.stdout).unwrap();
        let mut db = SourceDb::new();
        let mut sources = support::keel_ir::sources(&mut db, &metadata);
        let consumer = db.load_with_edition(&consumer, Edition::E2024).unwrap();
        sources.push(flux_hir::SourceCrate {
            root: consumer,
            name: flux_intern::Symbol::intern("consumer"),
            dependencies: vec![(flux_intern::Symbol::intern("keel_ir"), 2)],
        });
        let hir = flux_hir::lower_sources(&mut db, &sources);
        report(&root, "syntax", &db, &db.syntax_errors());
        report(&root, "hir", &db, &hir.diagnostics);
        assert!(
            db.syntax_errors().is_empty() && hir.diagnostics.is_empty(),
            "admission blockers; see {}",
            root.display()
        );
        let typed = flux_mir::check_file(&hir);
        report(&root, "type", &db, &typed.diagnostics);
        assert!(
            typed.diagnostics.is_empty(),
            "type blockers; see {}",
            root.display()
        );
        let mir = flux_mir::build_file(&hir, &typed);
        report(&root, "mir", &db, &mir.diagnostics);
        assert!(
            mir.diagnostics.is_empty(),
            "MIR blockers; see {}",
            root.display()
        );
        for (label, tier) in [
            ("off", flux_codegen::OptimizationTier::None),
            ("on", flux_codegen::OptimizationTier::Full),
        ] {
            let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
            let target = flux_codegen::Target::Aarch64AppleDarwin;
            let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
            let executable = root.join(format!("flux-{label}"));
            flux_codegen::link_executable(&object, target, &executable).unwrap();
            let actual = Command::new(executable).output().unwrap();
            assert_eq!(
                (actual.status.code(), actual.stdout, actual.stderr),
                (
                    expected.status.code(),
                    expected.stdout.clone(),
                    expected.stderr.clone()
                )
            );
        }
    }));
    let changed: Vec<_> = originals
        .iter()
        .filter(|(p, bytes)| std::fs::read(p).ok().as_ref() != Some(bytes))
        .map(|(p, _)| p)
        .collect();
    std::fs::write(
        root.join("source-integrity.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert!(changed.is_empty(), "audit inputs changed: {changed:?}");
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
