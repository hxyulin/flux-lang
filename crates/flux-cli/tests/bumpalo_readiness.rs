//! The unchanged allocator dependency and its resolved backend feature set.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use flux_hir::{CrateCfg, SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

fn inventory(path: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() { inventory(&path, files); }
        else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push((path.clone(), std::fs::read(path).unwrap()));
        }
    }
}

fn report(root: &Path, stage: &str, db: &SourceDb, errors: &[flux_hir::Diagnostic]) {
    let mut text = format!("count={}\n", errors.len());
    for error in errors {
        let file = db.file(error.file);
        let start = (error.range.start() as usize).min(file.text().len());
        let line = file.text().as_bytes()[..start].iter().filter(|&&b| b == b'\n').count() + 1;
        text.push_str(&format!("{}:{line}:{:?}: {}\n", file.path.display(), error.range, error.message));
    }
    std::fs::write(root.join(format!("{stage}.diagnostics")), text).unwrap();
}

#[test]
fn unchanged_bumpalo_matches_rustc() {
    if std::env::var_os("FLUX_BUMPALO_ROOT").is_some() { worker(); return; }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let root = repo.join(format!("out/bumpalo-readiness/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "unchanged_bumpalo_matches_rustc", "--nocapture"])
        .env("FLUX_BUMPALO_ROOT", &root)
        .stdout(Stdio::from(std::fs::File::create(root.join("audit.stdout")).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(root.join("audit.stderr")).unwrap()))
        .spawn().unwrap();
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() { break status; }
        if started.elapsed().as_secs() >= 300 { child.kill().unwrap(); break child.wait().unwrap(); }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    if let Ok(bytes) = std::fs::read(root.join("source-inventory.json")) {
        let originals: Vec<(PathBuf, Vec<u8>)> = serde_json::from_slice(&bytes).unwrap();
        let changed: Vec<_> = originals.iter().filter(|(p, bytes)| std::fs::read(p).ok().as_ref() != Some(bytes)).map(|(p, _)| p).collect();
        std::fs::write(root.join("parent-source-integrity.json"), serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(changed.is_empty(), "audit inputs changed: {changed:?}");
    }
    assert!(status.success(), "audit failed; see {}", root.display());
}

fn worker() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let root = PathBuf::from(std::env::var_os("FLUX_BUMPALO_ROOT").unwrap());
    let metadata = Command::new("cargo").current_dir(&repo)
        .args(["metadata", "--offline", "--locked", "--format-version", "1"]).output().unwrap();
    assert!(metadata.status.success());
    std::fs::write(root.join("metadata.json"), &metadata.stdout).unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let package = metadata["packages"].as_array().unwrap().iter().find(|p| p["name"] == "bumpalo").unwrap();
    assert_eq!(package["edition"], "2021");
    let node = metadata["resolve"]["nodes"].as_array().unwrap().iter().find(|n| n["id"] == package["id"]).unwrap();
    assert_eq!(node["features"], serde_json::json!(["collections", "default"]));
    assert!(node["deps"].as_array().unwrap().iter().all(|d| d["dep_kinds"].as_array().unwrap().iter().all(|k| !k["kind"].is_null())));
    let source = PathBuf::from(package["targets"].as_array().unwrap().iter().find(|t| t["kind"].as_array().unwrap().iter().any(|k| k == "lib")).unwrap()["src_path"].as_str().unwrap());
    let mut originals = Vec::new();
    inventory(source.parent().unwrap(), &mut originals);
    inventory(&repo.join("library"), &mut originals);
    for path in [repo.join("Cargo.lock"), repo.join("crates/flux-hir/src/source.rs"), PathBuf::from(package["manifest_path"].as_str().unwrap())] {
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }
    std::fs::write(root.join("source-inventory.json"), serde_json::to_vec(&originals).unwrap()).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let consumer = root.join("consumer.rs");
        std::fs::write(&consumer, include_str!("fixtures/bumpalo/consumer.rs.txt")).unwrap();
        let library = root.join("libbumpalo.rlib");
        let output = Command::new("rustc").args(["--edition=2021", "--crate-name=bumpalo", "--crate-type=rlib", "--cfg", "feature=\"collections\"", "--cfg", "feature=\"default\"", "-C", "panic=abort"])
            .arg(&source).arg("-o").arg(&library).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let reference = root.join("reference");
        let output = Command::new("rustc").args(["--edition=2024", "-C", "panic=abort"])
            .arg(&consumer).arg("--extern").arg(format!("bumpalo={}", library.display())).arg("-o").arg(&reference).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected = Command::new(reference).output().unwrap();
        assert!(expected.status.success());
        std::fs::write(root.join("reference.stdout"), &expected.stdout).unwrap();
        let mut db = SourceDb::new();
        let library = db.load_with_edition(&source, Edition::E2021).unwrap();
        db.set_crate_cfg(library, CrateCfg { names: vec![], values: ["collections", "default"].into_iter().map(|f| (Symbol::intern("feature"), Symbol::intern(f))).collect() }).unwrap();
        let consumer = db.load_with_edition(&consumer, Edition::E2024).unwrap();
        let hir = flux_hir::lower_sources(&mut db, &[
            SourceCrate { root: library, name: Symbol::intern("bumpalo"), dependencies: vec![] },
            SourceCrate { root: consumer, name: Symbol::intern("consumer"), dependencies: vec![(Symbol::intern("bumpalo"), 0)] },
        ]);
        report(&root, "syntax", &db, &db.syntax_errors());
        report(&root, "hir", &db, &hir.diagnostics);
        assert!(db.syntax_errors().is_empty() && hir.diagnostics.is_empty(), "admission blockers; see {}", root.display());
        let typed = flux_mir::check_file(&hir);
        report(&root, "type", &db, &typed.diagnostics);
        assert!(typed.diagnostics.is_empty(), "type blockers; see {}", root.display());
        let mir = flux_mir::build_file(&hir, &typed);
        report(&root, "mir", &db, &mir.diagnostics);
        assert!(mir.diagnostics.is_empty(), "MIR blockers; see {}", root.display());
        for (label, tier) in [("off", flux_codegen::OptimizationTier::None), ("on", flux_codegen::OptimizationTier::Full)] {
            let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
            let target = flux_codegen::Target::Aarch64AppleDarwin;
            let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
            let executable = root.join(format!("flux-{label}"));
            flux_codegen::link_executable(&object, target, &executable).unwrap();
            let actual = Command::new(executable).output().unwrap();
            assert_eq!((actual.status.code(), actual.stdout, actual.stderr), (expected.status.code(), expected.stdout.clone(), expected.stderr.clone()));
        }
    }));
    let changed: Vec<_> = originals.iter().filter(|(p, bytes)| std::fs::read(p).ok().as_ref() != Some(bytes)).map(|(p, _)| p).collect();
    std::fs::write(root.join("source-integrity.json"), serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(changed.is_empty(), "audit inputs changed: {changed:?}");
    if let Err(error) = result { std::panic::resume_unwind(error); }
}
