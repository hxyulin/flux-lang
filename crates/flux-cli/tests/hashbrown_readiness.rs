//! Audits the unchanged hashbrown 0.17.1 source graph used by flux-intern.
//! Features are disabled, matching flux-intern's dependency declaration.
//! Package resolution and upstream source rewrites are outside this probe.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

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
        .load_with_edition(&root.join("hashbrown-src/src/lib.rs"), Edition::E2024)
        .unwrap();
    let consumer = db
        .load_with_edition(&root.join("consumer.rs"), Edition::E2024)
        .unwrap();
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            SourceCrate {
                root: library,
                name: Symbol::intern("hashbrown"),
                dependencies: vec![],
            },
            SourceCrate {
                root: consumer,
                name: Symbol::intern("consumer"),
                dependencies: vec![(Symbol::intern("hashbrown"), 0)],
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
        "out/hashbrown-readiness/{label}-{}",
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
            if cfg!(target_arch = "x86_64") {
                "x86_64-unknown-linux-gnu"
            } else {
                "aarch64-apple-darwin"
            },
        ])
        .output()
        .unwrap();
    success(&metadata);
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "hashbrown" && package["version"] == "0.17.1")
        .unwrap();
    let source = Path::new(package["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap();
    let mut inventory = Vec::new();
    copy_tree(source, &root.join("hashbrown-src"), &mut inventory);
    std::fs::write(
        root.join("consumer.rs"),
        include_str!("fixtures/hashbrown/consumer.rs.txt"),
    )
    .unwrap();
    (root, inventory)
}

fn reference(root: &Path, optimized: bool) -> Output {
    let library = root.join("libhashbrown.rlib");
    let mut command = Command::new("rustc");
    command
        .args([
            "--edition=2024",
            "--crate-name=hashbrown",
            "--crate-type=rlib",
            "-C",
            "panic=abort",
            "-C",
            "overflow-checks=off",
        ])
        .arg(root.join("hashbrown-src/src/lib.rs"))
        .arg("-o")
        .arg(&library);
    if optimized {
        command.arg("-O");
    }
    success(&command.output().unwrap());
    let executable = root.join(format!("rustc-{optimized}"));
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
        .arg(format!("hashbrown={}", library.display()))
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
fn unchanged_hashbrown_source_audit_and_rustc_oracle() {
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
        "hashbrown source audit: {} HIR diagnostics; reports in {}",
        hir.diagnostics.len(),
        root.display()
    );
    unchanged(&inventory);
}

#[test]
fn unchanged_hashbrown_executes_with_flux_off_and_on() {
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
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        let target = flux_codegen::Target::host().unwrap();
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
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
fn hashbrown_prerequisites_match_rustc_off_and_on() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repository.join(format!(
        "out/hashbrown-readiness/prerequisites-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let source = repository.join("tests/run/hashbrown_prerequisites.rs");
    compare_program(&root, &source);
    compare_program(&root, &repository.join("tests/run/callable_references.rs"));
    compare_program(
        &root,
        &repository.join("tests/run/opaque_callback_storage.rs"),
    );
    compare_program(&root, &repository.join("tests/run/borrowed_fn_pointers.rs"));
}

fn compare_program(root: &Path, source: &Path) {
    let executable = root.join("rustc");
    let oracle = Command::new("rustc")
        .args([
            "--edition=2024",
            "-C",
            "panic=abort",
            "-C",
            "overflow-checks=off",
        ])
        .arg(source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    success(&oracle);
    let expected = execute(&executable);
    success(&expected);
    for optimized in [false, true] {
        let executable = root.join(format!("flux-{optimized}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        command.arg("build").arg(source).arg("-o").arg(&executable);
        if optimized {
            command.arg("-O");
        }
        success(&command.output().unwrap());
        let actual = execute(&executable);
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}

#[test]
fn unchanged_hashbrown_tag_source_matches_rustc_off_and_on() {
    let (root, inventory) = prepare("tag");
    let source = root.join("tag-consumer.rs");
    std::fs::write(
        &source,
        format!(
            "#[path = {:?}] mod tag;\n{}",
            root.join("hashbrown-src/src/control/tag.rs"),
            include_str!("fixtures/hashbrown/tag.rs.txt")
        ),
    )
    .unwrap();
    compare_program(&root, &source);
    unchanged(&inventory);
}
