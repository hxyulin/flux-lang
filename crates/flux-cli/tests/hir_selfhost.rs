//! Complete unchanged HIR source graph. Package resolution is a test input.
//! Each dependency retains its declared edition and resolved workspace features.
//! Original source bytes are inventoried before and after both compilers run.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use flux_hir::{CrateCfg, SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

fn success(output: &Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr).lines().filter(|line| line.starts_with("error") || line.starts_with("   -->")).take(15).collect::<Vec<_>>().join("\n"));
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
    report(root, label, db, errors);
    assert!(errors.is_empty(), "see {}", root.join(format!("{label}.diagnostics")).display());
}
fn report(root: &Path, label: &str, db: &SourceDb, errors: &[flux_hir::Diagnostic]) {
    let mut text = format!("count={}\n", errors.len());
    for error in errors {
        text.push_str(&format!("{}:{:?}: {}\n", db.file(error.file).path.display(), error.range, error.message));
    }
    std::fs::write(root.join(format!("{label}.diagnostics")), text).unwrap();
}
#[test]
fn unchanged_full_hir_matches_rustc() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let root = repo.join(format!("out/hir-selfhost/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let metadata = Command::new("cargo").current_dir(&repo).args(["metadata", "--offline", "--locked", "--format-version", "1"]).output().unwrap();
    success(&metadata);
    std::fs::write(root.join("metadata.json"), &metadata.stdout).unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let dependency = |name: &str, version: &str| {
        let package = metadata["packages"].as_array().unwrap().iter()
            .find(|package| package["name"] == name && package["version"] == version).unwrap();
        Path::new(package["manifest_path"].as_str().unwrap()).parent().unwrap().join("src/lib.rs")
    };
    let driver = root.join("consumer.rs");
    std::fs::write(&driver, include_str!("fixtures/hir/consumer.rs.txt")).unwrap();
    let cases = root.join("cases");
    std::fs::create_dir_all(&cases).unwrap();
    std::fs::write(root.join("inputs.rs"), format!("pub const ROOT: &str = {:?};\n", cases.join("main.rs").to_str().unwrap())).unwrap();
    std::fs::write(cases.join("main.rs"), r#"
mod child;
include!("included.rs");
const TEXT: &str = include_str!("message.txt");
#[cfg(feature = "enabled")] use child::Value;
#[cfg(feature = "enabled")] fn selected() { let _ = Value(7); }
#[cfg(not(feature = "enabled"))] fn selected() { child::value(); }
fn main() { selected(); made(); let _ = TEXT; }
"#).unwrap();
    std::fs::write(cases.join("child.rs"), "pub struct Value(pub u32); pub fn value() -> u32 { 11 }\n").unwrap();
    std::fs::write(cases.join("included.rs"), "macro_rules! make { ($name:ident) => { fn $name() {} }; } make!(made);\n").unwrap();
    std::fs::write(cases.join("message.txt"), "physical include: λ\r\n").unwrap();
    let roots = [
        repo.join("crates/flux-arena/src/lib.rs"),
        dependency("hashbrown", "0.17.1"),
        dependency("rustc-hash", "2.1.3"),
        repo.join("crates/flux-intern/src/lib.rs"),
        dependency("memchr", "2.8.3"),
        dependency("unicode-ident", "1.0.27"),
        repo.join("crates/flux-syntax/src/lib.rs"),
        repo.join("crates/flux-hir/src/lib.rs"),
        driver,
    ];
    let names = ["flux_arena", "hashbrown", "rustc_hash", "flux_intern", "memchr", "unicode_ident", "flux_syntax", "flux_hir", "consumer"];
    let dependencies: [&[usize]; 9] = [&[], &[], &[], &[0, 1, 2], &[], &[], &[4, 5], &[0, 2, 3, 6], &[3, 6, 7]];
    let editions = [Edition::E2024, Edition::E2024, Edition::E2021, Edition::E2024, Edition::E2021, Edition::E2021, Edition::E2024, Edition::E2024, Edition::E2024];
    let features: [&[&str]; 9] = [&[], &[], &["default", "std"], &[], &["alloc", "default", "std"], &[], &[], &[], &[]];
    for index in [2, 4] {
        let package = metadata["packages"].as_array().unwrap().iter().find(|p| p["name"].as_str().unwrap().replace('-', "_") == names[index]).unwrap();
        let node = metadata["resolve"]["nodes"].as_array().unwrap().iter().find(|n| n["id"] == package["id"]).unwrap();
        assert_eq!(node["features"], serde_json::json!(features[index]));
    }
    std::fs::write(root.join("configuration.txt"), format!("names={names:?}\neditions={editions:?}\nfeatures={features:?}\ndependencies={dependencies:?}\n")).unwrap();
    let mut originals = Vec::new();
    for source in &roots[..8] { inventory(source.parent().unwrap(), &mut originals); }
    inventory(&repo.join("library"), &mut originals);
    originals.sort_by(|a, b| a.0.cmp(&b.0));
    originals.dedup_by(|a, b| a.0 == b.0);
    std::fs::write(root.join("source-inventory.json"), serde_json::to_vec_pretty(
        &originals.iter().map(|(path, bytes)| (path, bytes.len())).collect::<Vec<_>>()
    ).unwrap()).unwrap();
    // rustc is an execution oracle only; Flux loads the same source roots.
    let mut libraries: Vec<PathBuf> = Vec::new();
    for index in 0..8 {
        let library = root.join(format!("lib{}.rlib", names[index]));
        let mut command = Command::new("rustc");
        command.args([if index == 2 || index == 4 || index == 5 { "--edition=2021" } else { "--edition=2024" }, "--crate-type=rlib", "-C", "panic=abort", "-C", "overflow-checks=off", "--crate-name"])
            .arg(names[index]).arg(&roots[index]).arg("-L").arg(&root).arg("-o").arg(&library);
        for feature in features[index] { command.arg("--cfg").arg(format!("feature=\"{feature}\"")); }
        for &dep in dependencies[index] { command.arg("--extern").arg(format!("{}={}", names[dep], libraries[dep].display())); }
        success(&bounded(&mut command, &root, &format!("rustc-lib-{index}")));
        libraries.push(library);
    }
    let reference = root.join("reference");
    let mut command = Command::new("rustc");
    command.args(["--edition=2024", "-C", "panic=abort", "-C", "overflow-checks=off"])
        .arg(&roots[8]).arg("-L").arg(&root).arg("-o").arg(&reference);
    for &dep in dependencies[8] { command.arg("--extern").arg(format!("{}={}", names[dep], libraries[dep].display())); }
    success(&bounded(&mut command, &root, "rustc-consumer"));
    let expected = bounded(&mut Command::new(&reference), &root, "run-reference");
    success(&expected);
    let mut db = SourceDb::new();
    let sources: Vec<_> = roots.iter().enumerate().map(|(index, source)| SourceCrate {
        root: db.load_with_edition(source, editions[index]).unwrap(), name: Symbol::intern(names[index]),
        dependencies: dependencies[index].iter().map(|&dep| (Symbol::intern(names[dep]), dep)).collect(),
    }).collect();
    for (index, source) in sources.iter().enumerate() {
        db.set_crate_cfg(source.root, CrateCfg { names: vec![], values: features[index].iter().map(|feature| (Symbol::intern("feature"), Symbol::intern(feature))).collect() }).unwrap();
    }
    let hir = flux_hir::lower_sources(&mut db, &sources);
    record(&root, "syntax", &db, &db.syntax_errors());
    // Keep the original source inventory checked even when an audit finds blockers.
    for (path, bytes) in &originals { assert_eq!(&std::fs::read(path).unwrap(), bytes, "{} changed", path.display()); }
    report(&root, "hir", &db, &hir.diagnostics);
    if std::env::var_os("FLUX_HIR_PROVISIONAL_TYPES").is_some()
        && !hir.diagnostics.is_empty() && hir.diagnostics.iter().all(|d| {
        d.message == "impl Trait in edition 2021 are not supported yet"
            || d.message == "higher-ranked bounds are not supported yet"
            || d.message.starts_with("cannot find")
            || d.message.starts_with("unresolved import `std::")
    }) {
        // Read-only inventory beneath known source/API admission diagnostics.
        // These results are provisional. Optional MIR inspection skips tainted
        // bodies; the full MIR builder and codegen require a clean graph below.
        let typed = flux_mir::check_file(&hir);
        report(&root, "provisional-type", &db, &typed.diagnostics);
        if std::env::var_os("FLUX_HIR_PROVISIONAL_MIR").is_some() {
            let mut errors = hir.diagnostics.clone();
            errors.extend(typed.diagnostics.iter().cloned());
            let diagnostics = flux_mir::check_clean_bodies(&hir, &typed, &errors);
            report(&root, "provisional-mir", &db, &diagnostics);
        }
    }
    record(&root, "hir", &db, &hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    report(&root, "type", &db, &typed.diagnostics);
    if !typed.diagnostics.is_empty() && std::env::var_os("FLUX_HIR_PROVISIONAL_MIR").is_some() {
        let diagnostics = flux_mir::check_clean_bodies(&hir, &typed, &typed.diagnostics);
        report(&root, "provisional-mir", &db, &diagnostics);
    }
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
