//! Audits the unchanged smallvec 1.16.3 source graph used by Flux MIR/codegen and Keel.
//! Workspace metadata selects no optional features. Both compilers consume
//! the original edition-2018 library and an edition-2024 consumer.
//! Package resolution and upstream source rewrites are outside this probe.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use flux_hir::{Diagnostic, SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn execute(executable: &Path) -> Output {
    let stdout = executable.with_extension("stdout");
    let stderr = executable.with_extension("stderr");
    let mut child = Command::new(executable)
        .env_remove("RUST_BACKTRACE")
        .stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap()))
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(20) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{} timed out", executable.display());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: std::fs::read(stdout).unwrap(),
        stderr: std::fs::read(stderr).unwrap(),
    }
}

fn copy_tree(source: &Path, target: &Path, inventory: &mut Vec<(PathBuf, Vec<u8>)>) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target, inventory);
        } else {
            let bytes = std::fs::read(entry.path()).unwrap();
            std::fs::write(&target, &bytes).unwrap();
            inventory.push((entry.path(), bytes.clone()));
            inventory.push((target, bytes));
        }
    }
}

fn record(root: &Path, name: &str, db: &SourceDb, diagnostics: &[Diagnostic]) {
    let mut text = format!("count={}\n", diagnostics.len());
    for diagnostic in diagnostics {
        text.push_str(&format!(
            "{}:{:?}: {}\n",
            db.file(diagnostic.file).path.display(),
            diagnostic.range,
            diagnostic.message
        ));
    }
    std::fs::write(root.join(format!("{name}.diagnostics")), text).unwrap();
}

fn graph(root: &Path) -> (SourceDb, flux_hir::Hir) {
    let mut db = SourceDb::new();
    let library = db
        .load_with_edition(&root.join("smallvec-src/src/lib.rs"), Edition::E2018)
        .unwrap();
    let consumer = db
        .load_with_edition(&root.join("consumer.rs"), Edition::E2024)
        .unwrap();
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            SourceCrate {
                root: library,
                name: Symbol::intern("smallvec"),
                dependencies: vec![],
            },
            SourceCrate {
                root: consumer,
                name: Symbol::intern("consumer"),
                dependencies: vec![(Symbol::intern("smallvec"), 0)],
            },
        ],
    );
    record(root, "syntax", &db, &db.syntax_errors());
    record(root, "hir", &db, &hir.diagnostics);
    assert!(db.syntax_errors().is_empty(), "{:?}", db.syntax_errors());
    (db, hir)
}

fn prepare(label: &str) -> (PathBuf, Vec<(PathBuf, Vec<u8>)>) {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repository.join(format!(
        "out/smallvec-readiness/{label}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let metadata = Command::new("cargo")
        .current_dir(&repository)
        .args([
            "metadata",
            "--offline",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            "aarch64-apple-darwin",
        ])
        .output()
        .unwrap();
    success(&metadata);
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "smallvec" && package["version"] == "1.16.3")
        .unwrap();
    let resolved = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == package["id"])
        .unwrap();
    assert_eq!(
        resolved["features"],
        serde_json::json!([]),
        "update the source graph feature configuration when the workspace changes"
    );
    std::fs::write(root.join("features.json"), serde_json::to_string_pretty(&serde_json::json!({"version": package["version"], "edition": package["edition"], "resolved": resolved["features"], "declared": package["features"]})).unwrap()).unwrap();
    let source = Path::new(package["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap();
    let mut inventory = Vec::new();
    copy_tree(source, &root.join("smallvec-src"), &mut inventory);
    std::fs::write(
        root.join("consumer.rs"),
        include_str!("fixtures/smallvec/consumer.rs.txt"),
    )
    .unwrap();
    (root, inventory)
}

fn reference(root: &Path, optimized: bool) -> Output {
    reference_features(root, optimized, &[])
}

fn reference_features(root: &Path, optimized: bool, features: &[&str]) -> Output {
    let library = root.join("libsmallvec.rlib");
    let mut command = Command::new("rustc");
    command
        .args([
            "--edition=2018",
            "--crate-name=smallvec",
            "--crate-type=rlib",
            "-C",
            "panic=abort",
            "-C",
            "overflow-checks=off",
        ])
        .arg(root.join("smallvec-src/src/lib.rs"))
        .arg("-o")
        .arg(&library);
    if optimized {
        command.arg("-O");
    }
    for feature in features {
        command.arg("--cfg").arg(format!("feature=\"{feature}\""));
    }
    success(&command.output().unwrap());
    let executable = root.join(format!("rustc-{}-{optimized}", features.join("-")));
    let mut command = Command::new("rustc");
    command
        .args([
            "--edition=2024",
            "-C",
            "panic=abort",
            "-C",
            "overflow-checks=off",
        ])
        .arg(root.join("consumer.rs"))
        .arg("--extern")
        .arg(format!("smallvec={}", library.display()))
        .arg("-o")
        .arg(&executable);
    if optimized {
        command.arg("-O");
    }
    success(&command.output().unwrap());
    let output = execute(&executable);
    success(&output);
    std::fs::write(
        root.join(format!("rustc-{optimized}.stdout")),
        &output.stdout,
    )
    .unwrap();
    output
}

/// Optional layouts are audited independently of the resolved workspace graph.
/// This rustc oracle does not claim Flux support for unselected cfg branches.
#[test]
fn optional_storage_features_rustc_audit() {
    let (root, inventory) = prepare("optional-features");
    let expected = reference(&root, false);
    for features in [
        &["union"][..],
        &["const_generics"][..],
        &["union", "const_generics"][..],
    ] {
        let actual = reference_features(&root, false, features);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    unchanged(&inventory);
}

fn unchanged(inventory: &[(PathBuf, Vec<u8>)]) {
    for (path, bytes) in inventory {
        assert_eq!(
            &std::fs::read(path).unwrap(),
            bytes,
            "{} changed",
            path.display()
        );
    }
}

#[test]
fn unchanged_smallvec_source_audit_and_rustc_oracle() {
    let (root, inventory) = prepare("audit");
    let unoptimized = reference(&root, false);
    let optimized = reference(&root, true);
    assert_eq!(unoptimized.stdout, optimized.stdout);
    let (db, hir) = graph(&root);
    // Checking incomplete source gathers evidence only. Native compilation
    // below still requires every stage's diagnostics to be empty.
    let typed = flux_mir::check_file(&hir);
    record(&root, "type", &db, &typed.diagnostics);
    eprintln!(
        "smallvec source audit: {} HIR diagnostics; reports in {}",
        hir.diagnostics.len(),
        root.display()
    );
    unchanged(&inventory);
}

#[test]
fn unchanged_smallvec_executes_with_flux_off_and_on() {
    let (root, inventory) = prepare("native");
    let expected = reference(&root, false);
    let (db, hir) = graph(&root);
    assert!(
        hir.diagnostics.is_empty(),
        "see {}",
        root.join("hir.diagnostics").display()
    );
    let typed = flux_mir::check_file(&hir);
    record(&root, "type", &db, &typed.diagnostics);
    assert!(
        typed.diagnostics.is_empty(),
        "see {}",
        root.join("type.diagnostics").display()
    );
    let mir = flux_mir::build_file(&hir, &typed);
    std::fs::write(root.join("source.mir"), mir.debug_dump(&hir)).unwrap();
    record(&root, "mir", &db, &mir.diagnostics);
    assert!(
        mir.diagnostics.is_empty(),
        "see {}",
        root.join("mir.diagnostics").display()
    );
    for (label, tier) in [
        ("off", flux_codegen::OptimizationTier::None),
        ("on", flux_codegen::OptimizationTier::Full),
    ] {
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_or_else(|error| {
            record(&root, "lower", &db, &error);
            panic!("see {}", root.join("lower.diagnostics").display());
        });
        let target = flux_codegen::Target::Aarch64AppleDarwin;
        let object =
            flux_codegen::compile_object(&mut module, target, tier).unwrap_or_else(|error| {
                let error = error.to_string();
                std::fs::write(root.join("codegen.errors"), &error).unwrap();
                panic!(
                    "{}\nsee {}",
                    error.lines().take(8).collect::<Vec<_>>().join("\n"),
                    root.join("codegen.errors").display()
                );
            });
        let executable = root.join(format!("flux-{label}"));
        flux_codegen::link_executable(&object, target, &executable).unwrap();
        let actual = execute(&executable);
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    unchanged(&inventory);
}

#[test]
fn smallvec_prerequisites_match_rustc_off_and_on() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repository.join(format!(
        "out/smallvec-readiness/prerequisites-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for name in [
        "smallvec_prerequisites",
        "type_alias_inference",
        "slice_ranges",
        "range_indexing",
        "slice_iterators",
        "range_out_of_bounds",
    ] {
        let source = repository.join(format!("tests/run/{name}.rs"));
        let executable = root.join(format!("{name}-rustc"));
        let reference = Command::new("rustc")
            .args([
                "--edition=2024",
                "-C",
                "panic=abort",
                "-C",
                "overflow-checks=off",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        success(&reference);
        let expected = execute(&executable);
        for optimized in [false, true] {
            let executable = root.join(format!("{name}-flux-{optimized}"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command.arg("build").arg(&source).arg("-o").arg(&executable);
            if optimized {
                command.arg("-O");
            }
            success(&command.output().unwrap());
            let actual = execute(&executable);
            assert_eq!(actual.status, expected.status, "{name}");
            assert_eq!(actual.stdout, expected.stdout, "{name}");
            assert_eq!(
                normalize_panic(&actual.stderr),
                normalize_panic(&expected.stderr),
                "{name}"
            );
        }
    }
}

fn normalize_panic(stderr: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(stderr).into_owned();
    let marker = "thread 'main' (";
    if let Some(start) = text.find(marker) {
        let start = start + marker.len();
        let digits = text[start..].bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && text[start + digits..].starts_with(')') {
            text.replace_range(start..start + digits, "N");
        }
    }
    text
}
