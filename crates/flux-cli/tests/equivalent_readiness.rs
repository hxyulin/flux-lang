//! Unchanged equivalent 1.0.3 in its original edition 2015, with a modern consumer.
mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn bounded(command: &mut Command, directory: &Path, label: &str) -> Output {
    support::process::run(command, directory, label, Duration::from_secs(30)).completed()
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn original_equivalent_2015_matches_rustc_off_and_on() {
    use flux_hir::{SourceCrate, SourceDb};
    use flux_intern::Symbol;
    use flux_syntax::Edition;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repository.join(format!("out/equivalent-readiness/{}", std::process::id()));
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
        .find(|p| p["name"] == "equivalent" && p["version"] == "1.0.3")
        .unwrap();
    assert_eq!(package["edition"], "2015");
    let node = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == package["id"])
        .unwrap();
    assert_eq!(node["features"], serde_json::json!([]));
    assert!(node["deps"].as_array().unwrap().is_empty());
    let manifest = Path::new(package["manifest_path"].as_str().unwrap());
    let source = manifest.parent().unwrap().join("src/lib.rs");
    let inventory = vec![
        (manifest.to_path_buf(), std::fs::read(manifest).unwrap()),
        (source.clone(), std::fs::read(&source).unwrap()),
    ];
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
    let consumer = root.join("consumer.rs");
    std::fs::write(
        &consumer,
        include_str!("fixtures/equivalent/consumer.rs.txt"),
    )
    .unwrap();
    std::fs::write(root.join("configuration.json"),serde_json::to_vec_pretty(&serde_json::json!({"library":source,"edition":"2015","features":[],"dependencies":[],"consumer_edition":"2024"})).unwrap()).unwrap();
    let library = root.join("libequivalent.rlib");
    let output = bounded(
        Command::new("rustc")
            .args([
                "--edition=2015",
                "--crate-name=equivalent",
                "--crate-type=rlib",
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
            .arg(format!("equivalent={}", library.display()))
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
    assert_eq!(expected.stdout, b"equivalent 3 4 2 120\n");
    let mut db = SourceDb::new();
    let library = db.load_with_edition(&source, Edition::E2015).unwrap();
    let main = db.load_with_edition(&consumer, Edition::E2024).unwrap();
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            SourceCrate {
                root: library,
                name: Symbol::intern("equivalent"),
                dependencies: vec![],
            },
            SourceCrate {
                root: main,
                name: Symbol::intern("consumer"),
                dependencies: vec![(Symbol::intern("equivalent"), 0)],
            },
        ],
    );
    let record = |label: &str, errors: &[flux_hir::Diagnostic]| {
        let mut report = format!("count={}\n", errors.len());
        for d in errors {
            report.push_str(&format!(
                "{}:{:?}: {}\n",
                db.file(d.file).path.display(),
                d.range,
                d.message
            ));
        }
        std::fs::write(root.join(format!("{label}.diagnostics")), report).unwrap();
        assert!(errors.is_empty(), "{label}: {errors:?}");
    };
    record("syntax", &db.syntax_errors());
    record("hir", &hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    record("type", &typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    record("mir", &mir.diagnostics);
    for optimized in [false, true] {
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        let target = flux_codegen::Target::Aarch64AppleDarwin;
        let tier = if optimized {
            flux_codegen::OptimizationTier::Full
        } else {
            flux_codegen::OptimizationTier::None
        };
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
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
