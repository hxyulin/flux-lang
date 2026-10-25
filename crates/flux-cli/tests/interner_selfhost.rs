//! Unchanged arena, hashbrown, rustc-hash and complete interner source graph.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

fn success(output: &Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
fn bounded(command: &mut Command, directory: &Path, label: &str) -> Output {
    let out = directory.join(format!("{label}.stdout"));
    let err = directory.join(format!("{label}.stderr"));
    let mut child = command.stdout(Stdio::from(std::fs::File::create(&out).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&err).unwrap())).spawn().unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() { break status; }
        if start.elapsed() > Duration::from_secs(60) {
            child.kill().unwrap(); child.wait().unwrap(); panic!("{label} timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output { status, stdout: std::fs::read(out).unwrap(), stderr: std::fs::read(err).unwrap() }
}
fn inventory(directory: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() { inventory(&entry.path(), files); }
        else if entry.path().extension().is_some_and(|ext| ext == "rs") {
            files.push((entry.path(), std::fs::read(entry.path()).unwrap()));
        }
    }
}
fn record(root: &Path, label: &str, db: &SourceDb, errors: &[flux_hir::Diagnostic]) {
    let mut text = format!("count={}\n", errors.len());
    for error in errors {
        text.push_str(&format!("{}:{:?}: {}\n", db.file(error.file).path.display(), error.range, error.message));
    }
    std::fs::write(root.join(format!("{label}.diagnostics")), text).unwrap();
    assert!(errors.is_empty(), "see {}", root.join(format!("{label}.diagnostics")).display());
}
#[test]
fn unchanged_full_interner_matches_rustc() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let root = repo.join(format!("out/interner-selfhost/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let metadata = Command::new("cargo").current_dir(&repo).args(["metadata", "--offline", "--locked", "--format-version", "1"]).output().unwrap();
    success(&metadata);
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let dependency = |name: &str, version: &str| {
        let package = metadata["packages"].as_array().unwrap().iter()
            .find(|package| package["name"] == name && package["version"] == version).unwrap();
        Path::new(package["manifest_path"].as_str().unwrap()).parent().unwrap().join("src/lib.rs")
    };
    let driver = root.join("consumer.rs");
    std::fs::write(&driver, include_str!("fixtures/interner/consumer.rs.txt")).unwrap();
    let roots = [
        repo.join("crates/flux-arena/src/lib.rs"),
        dependency("hashbrown", "0.17.1"),
        dependency("rustc-hash", "2.1.3"),
        repo.join("crates/flux-intern/src/lib.rs"),
        driver,
    ];
    let names = ["flux_arena", "hashbrown", "rustc_hash", "flux_intern", "consumer"];
    let dependencies: [&[usize]; 5] = [&[], &[], &[], &[0, 1, 2], &[3]];
    let mut originals = Vec::new();
    for source in &roots[..4] { inventory(source.parent().unwrap(), &mut originals); }
    // rustc is an execution oracle only; Flux loads the same source roots.
    let mut libraries: Vec<PathBuf> = Vec::new();
    for index in 0..4 {
        let library = root.join(format!("lib{}.rlib", names[index]));
        let mut command = Command::new("rustc");
        command.args(["--edition=2024", "--crate-type=rlib", "-C", "panic=abort", "-C", "overflow-checks=off", "--crate-name"])
            .arg(names[index]).arg(&roots[index]).arg("-L").arg(&root).arg("-o").arg(&library);
        for &dep in dependencies[index] { command.arg("--extern").arg(format!("{}={}", names[dep], libraries[dep].display())); }
        success(&bounded(&mut command, &root, &format!("rustc-lib-{index}")));
        libraries.push(library);
    }
    let reference = root.join("reference");
    success(&bounded(Command::new("rustc").args(["--edition=2024", "-C", "panic=abort", "-C", "overflow-checks=off"])
        .arg(&roots[4]).arg("--extern").arg(format!("flux_intern={}", libraries[3].display()))
        .arg("-L").arg(&root).arg("-o").arg(&reference), &root, "rustc-consumer"));
    let expected = bounded(&mut Command::new(&reference), &root, "run-reference");
    success(&expected);
    let mut db = SourceDb::new();
    let sources: Vec<_> = roots.iter().enumerate().map(|(index, source)| SourceCrate {
        root: db.load_with_edition(source, Edition::E2024).unwrap(), name: Symbol::intern(names[index]),
        dependencies: dependencies[index].iter().map(|&dep| (Symbol::intern(names[dep]), dep)).collect(),
    }).collect();
    let hir = flux_hir::lower_sources(&mut db, &sources);
    record(&root, "syntax", &db, &db.syntax_errors());
    record(&root, "hir", &db, &hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    record(&root, "type", &db, &typed.diagnostics);
    assert!(flux_ty::check_main(&hir.items, &typed.sigs).is_none());
    let mir = flux_mir::build_file(&hir, &typed);
    record(&root, "mir", &db, &mir.diagnostics);
    for (optimized, tier) in [(false, flux_codegen::OptimizationTier::None), (true, flux_codegen::OptimizationTier::Full)] {
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        let target = flux_codegen::Target::Aarch64AppleDarwin;
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
        let executable = root.join(format!("flux-{optimized}"));
        flux_codegen::link_executable(&object, target, &executable).unwrap();
        let actual = bounded(&mut Command::new(&executable), &root, &format!("run-flux-{optimized}"));
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    for (path, bytes) in originals { assert_eq!(std::fs::read(&path).unwrap(), bytes, "{} changed", path.display()); }
}
