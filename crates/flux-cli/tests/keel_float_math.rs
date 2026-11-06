//! Native differential of unchanged pinned Keel float-folding source and IR types.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::path::{Path, PathBuf};
use std::process::Command;
fn inventory(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let p = entry.path();
        if entry.file_type().unwrap().is_dir() {
            inventory(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push((p.clone(), std::fs::read(p).unwrap()));
        }
    }
}
fn report(root: &Path, name: &str, db: &flux_hir::SourceDb, errors: &[flux_hir::Diagnostic]) {
    let mut text = format!("count={}\n", errors.len());
    for error in errors {
        let file = db.file(error.file);
        let bytes = file.text().as_bytes();
        let line = bytes[..(error.range.start() as usize).min(bytes.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1;
        text.push_str(&format!(
            "{}:{line}:{:?}: {}\n",
            file.path.display(),
            error.range,
            error.message
        ));
    }
    std::fs::write(root.join(format!("{name}.diagnostics")), text).unwrap();
}
#[test]
fn unchanged_keel_float_folding_matches_rustc_default_environment() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/keel-float-math/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let output = Command::new("cargo")
        .current_dir(&repo)
        .args(["metadata", "--offline", "--locked", "--format-version", "1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    std::fs::write(root.join("metadata.json"), &output.stdout).unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let package = packages.iter().find(|p| p["name"] == "keel-ir").unwrap();
    assert!(
        package["source"]
            .as_str()
            .unwrap()
            .ends_with("#f503b7263f5ea2d85367a4e986027fea8ab9ce20")
    );
    let opt = packages.iter().find(|p| p["name"] == "keel-opt").unwrap();
    let fp = PathBuf::from(opt["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap()
        .join("src/const_fold/fp.rs");
    let mut originals = vec![(fp.clone(), std::fs::read(&fp).unwrap())];
    for dependency in support::keel_ir::packages(&metadata) {
        inventory(
            support::keel_ir::source(dependency).parent().unwrap(),
            &mut originals,
        );
        let manifest = PathBuf::from(dependency["manifest_path"].as_str().unwrap());
        originals.push((manifest.clone(), std::fs::read(manifest).unwrap()));
    }
    inventory(&repo.join("library"), &mut originals);
    originals.push((
        repo.join("crates/flux-hir/src/source.rs"),
        std::fs::read(repo.join("crates/flux-hir/src/source.rs")).unwrap(),
    ));
    std::fs::write(
        root.join("source-inventory.json"),
        serde_json::to_vec(&originals).unwrap(),
    )
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let consumer = root.join("consumer.rs");
        std::fs::write(
            &consumer,
            include_str!("fixtures/keel_float_math/consumer.rs.txt")
                .replace("__FLOAT_SOURCE__", fp.to_str().unwrap()),
        )
        .unwrap();
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
        std::fs::write(root.join("rustc.stdout"), &expected.stdout).unwrap();
        let mut db = flux_hir::SourceDb::new();
        let mut sources = support::keel_ir::sources(&mut db, &metadata);
        let consumer = db.load(&consumer).unwrap();
        sources.push(flux_hir::SourceCrate {
            root: consumer,
            name: flux_intern::Symbol::intern("consumer"),
            dependencies: vec![(flux_intern::Symbol::intern("keel_ir"), 2)],
        });
        let hir = flux_hir::lower_sources(&mut db, &sources);
        let files: Vec<_> = db
            .files()
            .map(|(id, file)| (id.0, file.path.clone()))
            .collect();
        std::fs::write(
            root.join("loaded-files.json"),
            serde_json::to_vec_pretty(&files).unwrap(),
        )
        .unwrap();
        for (name, errors) in [
            ("syntax", db.syntax_errors()),
            ("hir", hir.diagnostics.clone()),
        ] {
            report(&root, name, &db, &errors);
            assert!(errors.is_empty(), "{name}: see {}", root.display());
        }
        let typed = flux_mir::check_file(&hir);
        report(&root, "type", &db, &typed.diagnostics);
        assert!(typed.diagnostics.is_empty(), "type: see {}", root.display());
        let mir = flux_mir::build_file(&hir, &typed);
        report(&root, "mir", &db, &mir.diagnostics);
        assert!(mir.diagnostics.is_empty(), "MIR: see {}", root.display());
        for (name, tier) in [
            ("off", flux_codegen::OptimizationTier::None),
            ("on", flux_codegen::OptimizationTier::Full),
        ] {
            let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
            let target = flux_codegen::Target::Aarch64AppleDarwin;
            let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
            let executable = root.join(format!("flux-{name}"));
            flux_codegen::link_executable(&object, target, &executable).unwrap();
            let actual = Command::new(executable).output().unwrap();
            std::fs::write(root.join(format!("flux-{name}.stdout")), &actual.stdout).unwrap();
            std::fs::write(root.join(format!("flux-{name}.stderr")), &actual.stderr).unwrap();
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
        .filter(|(p, b)| std::fs::read(p).ok().as_ref() != Some(b))
        .map(|(p, _)| p)
        .collect();
    std::fs::write(
        root.join("source-integrity.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert!(changed.is_empty(), "source inputs changed: {changed:?}");
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
