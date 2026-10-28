//! Unchanged typenum 1.20.1 in its original edition 2018, with a modern consumer.
mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn bounded(command: &mut Command, directory: &Path, label: &str) -> Output {
    support::process::run(command, directory, label, Duration::from_secs(30)).completed()
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn original_typenum_2018_matches_rustc_off_and_on() {
    if std::env::var_os("FLUX_TYPENUM_ROOT").is_some() {
        audit();
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/typenum-readiness/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let output = support::process::run(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "original_typenum_2018_matches_rustc_off_and_on",
                "--nocapture",
            ])
            .env("FLUX_TYPENUM_ROOT", &root),
        &root,
        "audit",
        Duration::from_secs(300),
    )
    .output;
    let snapshot = root.join("input-bytes.json");
    if snapshot.exists() {
        let files: support::inventory::Files =
            serde_json::from_slice(&std::fs::read(snapshot).unwrap()).unwrap();
        let changed = support::inventory::changed(&files);
        std::fs::write(
            root.join("parent-source-integrity.json"),
            serde_json::to_vec_pretty(&changed).unwrap(),
        )
        .unwrap();
        assert!(changed.is_empty(), "inputs changed: {changed:?}");
    }
    assert!(
        output.status.success(),
        "typenum audit: {}; see {}",
        String::from_utf8_lossy(&output.stderr),
        root.display()
    );
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn audit() {
    use flux_hir::{CrateCfg, SourceCrate, SourceDb};
    use flux_intern::Symbol;
    use flux_syntax::Edition;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = std::path::PathBuf::from(std::env::var_os("FLUX_TYPENUM_ROOT").unwrap());
    std::fs::create_dir_all(&root).unwrap();
    let metadata = Command::new("cargo")
        .current_dir(&repository)
        .args(["metadata", "--offline", "--locked", "--format-version", "1"])
        .output()
        .unwrap();
    assert!(
        metadata.status.success(),
        "{}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    std::fs::write(root.join("metadata.json"), &metadata.stdout).unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "typenum" && p["version"] == "1.20.1")
        .unwrap();
    assert_eq!(package["edition"], "2018");
    let node = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == package["id"])
        .unwrap();
    assert_eq!(node["features"], serde_json::json!(["const-generics"]));
    assert!(node["deps"].as_array().unwrap().is_empty());
    let manifest = Path::new(package["manifest_path"].as_str().unwrap());
    let source = manifest.parent().unwrap().join("src/lib.rs");
    let mut inventory = vec![(manifest.to_path_buf(), std::fs::read(manifest).unwrap())];
    support::inventory::collect(manifest.parent().unwrap(), &mut inventory);
    support::inventory::collect(&repository.join("library"), &mut inventory);
    for path in [
        repository.join("Cargo.lock"),
        repository.join("crates/flux-hir/src/source.rs"),
        repository.join("crates/flux-cli/tests/fixtures/typenum/consumer.rs.txt"),
    ] {
        inventory.push((path.clone(), std::fs::read(path).unwrap()));
    }
    std::fs::write(
        root.join("source-inventory.json"),
        serde_json::to_vec_pretty(
            &inventory
                .iter()
                .map(|(p, b)| (p, b.len()))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("input-bytes.json"),
        serde_json::to_vec(&inventory).unwrap(),
    )
    .unwrap();
    let consumer = root.join("consumer.rs");
    std::fs::write(&consumer, include_str!("fixtures/typenum/consumer.rs.txt")).unwrap();
    std::fs::write(root.join("configuration.json"),serde_json::to_vec_pretty(&serde_json::json!({"library":source,"edition":"2018","features":["const-generics"],"dependencies":[],"consumer_edition":"2024"})).unwrap()).unwrap();
    let library = root.join("libtypenum.rlib");
    let output = bounded(
        Command::new("rustc")
            .args([
                "--edition=2018",
                "--crate-name=typenum",
                "--crate-type=rlib",
                "--cfg=feature=\"const-generics\"",
                "-Cpanic=abort",
                "-o",
            ])
            .arg(&library)
            .arg(&source),
        &root,
        "compile-rustc-library",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference = root.join("reference");
    let output = bounded(
        Command::new("rustc")
            .args(["--edition=2024", "-Cpanic=abort", "--extern"])
            .arg(format!("typenum={}", library.display()))
            .arg(&consumer)
            .arg("-o")
            .arg(&reference),
        &root,
        "compile-rustc-consumer",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = bounded(&mut Command::new(reference), &root, "run-reference");
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    assert_eq!(
        expected.stdout,
        b"typenum [5, 8, 35, 2, 3, 104, 3, 6, 81, 4] -8\n"
    );
    let mut db = SourceDb::new();
    let library = db.load_with_edition(&source, Edition::E2018).unwrap();
    db.set_crate_cfg(
        library,
        CrateCfg {
            names: vec![],
            values: vec![(Symbol::intern("feature"), Symbol::intern("const-generics"))],
        },
    )
    .unwrap();
    let main = db.load_with_edition(&consumer, Edition::E2024).unwrap();
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            SourceCrate {
                root: library,
                name: Symbol::intern("typenum"),
                dependencies: vec![],
            },
            SourceCrate {
                root: main,
                name: Symbol::intern("consumer"),
                dependencies: vec![(Symbol::intern("typenum"), 0)],
            },
        ],
    );
    let record = |label: &str, errors: &[flux_hir::Diagnostic]| {
        support::inventory::report(&root, label, &db, errors);
        std::fs::write(root.join("phase.txt"), label).unwrap();
        assert!(
            errors.is_empty(),
            "{label}: {errors:?}; see {}",
            root.display()
        );
    };
    record("syntax", &db.syntax_errors());
    record("hir", &hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    record("type", &typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    record("mir", &mir.diagnostics);
    for optimized in [false, true] {
        std::fs::write(root.join("phase.txt"), format!("codegen-{optimized}\n")).unwrap();
        assert!(
            support::inventory::changed(&inventory).is_empty(),
            "input bytes changed before codegen"
        );
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_or_else(|error| {
            std::fs::write(root.join("codegen-error.txt"), format!("{error:?}")).unwrap();
            panic!(
                "codegen failed; see {}",
                root.join("codegen-error.txt").display()
            );
        });
        let target = flux_codegen::Target::Aarch64AppleDarwin;
        let tier = if optimized {
            flux_codegen::OptimizationTier::Full
        } else {
            flux_codegen::OptimizationTier::None
        };
        let object =
            flux_codegen::compile_object(&mut module, target, tier).unwrap_or_else(|error| {
                std::fs::write(root.join("codegen-error.txt"), error.to_string()).unwrap();
                panic!(
                    "object compilation failed; see {}",
                    root.join("codegen-error.txt").display()
                );
            });
        let executable = root.join(format!("flux-{optimized}"));
        flux_codegen::link_executable(&object, target, &executable).unwrap();
        let actual = bounded(
            &mut Command::new(executable),
            &root,
            &format!("run-flux-{optimized}"),
        );
        assert_eq!(
            actual.status.code(),
            expected.status.code(),
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    for (path, bytes) in inventory {
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "{} changed",
            path.display()
        );
    }
    std::fs::write(root.join("source-integrity.json"), "[]\n").unwrap();
    std::fs::write(root.join("phase.txt"), "verified\n").unwrap();
}
