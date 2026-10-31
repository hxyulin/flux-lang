//! Original crc32fast sources, including their real build-script cfg.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod support;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    let output = support::process::run(command, root, label, Duration::from_secs(60)).completed();
    assert!(
        output.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn unchanged_crc32fast_matches_rustc() {
    use flux_hir::{CrateCfg, SourceCrate, SourceDb};
    use flux_intern::Symbol;
    use flux_syntax::Edition;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/crc32fast-readiness/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let output = run(
        Command::new("cargo").current_dir(&repo).args([
            "metadata",
            "--offline",
            "--locked",
            "--format-version",
            "1",
        ]),
        &root,
        "metadata",
    );
    std::fs::write(root.join("metadata.json"), &output.stdout).unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let find = |name: &str, version: &str| {
        packages
            .iter()
            .find(|p| p["name"] == name && p["version"] == version)
            .unwrap()
    };
    let crc = find("crc32fast", "1.5.3");
    let cfg_if = find("cfg-if", "1.0.5");
    assert_eq!(crc["edition"], "2021");
    assert_eq!(cfg_if["edition"], "2018");
    let node = |package: &serde_json::Value| {
        metadata["resolve"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == package["id"])
            .unwrap()
    };
    assert_eq!(node(crc)["features"], serde_json::json!(["std"]));
    assert_eq!(node(cfg_if)["features"], serde_json::json!([]));
    let deps = node(crc)["deps"].as_array().unwrap();
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0]["pkg"], cfg_if["id"]);
    let crc_dir = Path::new(crc["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap();
    let cfg_dir = Path::new(cfg_if["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap();
    let mut originals = Vec::new();
    for dir in [
        crc_dir.join("src"),
        cfg_dir.join("src"),
        repo.join("library"),
    ] {
        support::inventory::collect(&dir, &mut originals);
    }
    for path in [
        crc_dir.join("build.rs"),
        crc_dir.join("Cargo.toml"),
        cfg_dir.join("Cargo.toml"),
        repo.join("Cargo.lock"),
        repo.join("crates/flux-hir/src/source.rs"),
        repo.join("crates/flux-cli/tests/fixtures/crc32fast/consumer.rs.txt"),
        repo.join("crates/flux-cli/tests/crc32fast_readiness.rs"),
    ] {
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }
    std::fs::write(
        root.join("source-inventory.json"),
        serde_json::to_vec(&originals).unwrap(),
    )
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let script = root.join("build-script");
        run(
            Command::new("rustc")
                .arg(crc_dir.join("build.rs"))
                .args(["--edition=2021", "-o"])
                .arg(&script),
            &root,
            "compile-build-script",
        );
        let built = run(
            Command::new(script).env("RUSTC", "rustc"),
            &root,
            "build-script",
        );
        let cfgs: Vec<_> = String::from_utf8(built.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("cargo:rustc-cfg=").map(str::to_owned))
            .collect();
        assert!(cfgs.iter().any(|cfg| cfg == "stable_arm_crc32_intrinsics"));
        assert!(cfgs.iter().any(|cfg| cfg == "stable_vpclmulqdq"));
        std::fs::write(
            root.join("configuration.json"),
            serde_json::to_vec_pretty(&cfgs).unwrap(),
        )
        .unwrap();
        let cfg_library = root.join("libcfg_if.rlib");
        run(
            Command::new("rustc")
                .arg(cfg_dir.join("src/lib.rs"))
                .args([
                    "--edition=2018",
                    "--crate-name=cfg_if",
                    "--crate-type=rlib",
                    "-Cpanic=abort",
                    "-o",
                ])
                .arg(&cfg_library),
            &root,
            "rustc-cfg-if",
        );
        let library = root.join("libcrc32fast.rlib");
        let mut command = Command::new("rustc");
        command
            .arg(crc_dir.join("src/lib.rs"))
            .args([
                "--edition=2021",
                "--crate-name=crc32fast",
                "--crate-type=rlib",
                "-Cpanic=abort",
                "--cfg",
                "feature=\"std\"",
                "--extern",
            ])
            .arg(format!("cfg_if={}", cfg_library.display()))
            .arg("-o")
            .arg(&library);
        for cfg in &cfgs {
            command.arg("--cfg").arg(cfg);
        }
        run(&mut command, &root, "rustc-crc32fast");
        let source = root.join("consumer.rs");
        std::fs::write(&source, include_str!("fixtures/crc32fast/consumer.rs.txt")).unwrap();
        let reference = root.join("rustc");
        run(
            Command::new("rustc")
                .arg(&source)
                .args(["--edition=2024", "-Cpanic=abort", "--extern"])
                .arg(format!("crc32fast={}", library.display()))
                .arg("-L")
                .arg(&root)
                .arg("-o")
                .arg(&reference),
            &root,
            "rustc-consumer",
        );
        let expected = run(&mut Command::new(reference), &root, "rustc-run");
        let mut db = SourceDb::new();
        let cfg_file = db
            .load_with_edition(&cfg_dir.join("src/lib.rs"), Edition::E2018)
            .unwrap();
        let crc_file = db
            .load_with_edition(&crc_dir.join("src/lib.rs"), Edition::E2021)
            .unwrap();
        db.set_crate_cfg(
            crc_file,
            CrateCfg {
                names: cfgs.iter().map(|name| Symbol::intern(name)).collect(),
                values: ["std"]
                    .iter()
                    .map(|feature| (Symbol::intern("feature"), Symbol::intern(feature)))
                    .collect(),
            },
        )
        .unwrap();
        let main = db.load_with_edition(&source, Edition::E2024).unwrap();
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                SourceCrate {
                    root: cfg_file,
                    name: Symbol::intern("cfg_if"),
                    dependencies: vec![],
                },
                SourceCrate {
                    root: crc_file,
                    name: Symbol::intern("crc32fast"),
                    dependencies: vec![(Symbol::intern("cfg_if"), 0)],
                },
                SourceCrate {
                    root: main,
                    name: Symbol::intern("consumer"),
                    dependencies: vec![(Symbol::intern("crc32fast"), 1)],
                },
            ],
        );
        let record = |label: &str, errors: &[flux_hir::Diagnostic]| {
            support::inventory::report(&root, label, &db, errors);
            assert!(
                errors.is_empty(),
                "{label}: {errors:?}; artifacts {}",
                root.display()
            );
        };
        std::fs::write(root.join("phase.txt"), "syntax-hir\n").unwrap();
        record("syntax", &db.syntax_errors());
        record("hir", &hir.diagnostics);
        std::fs::write(root.join("phase.txt"), "type\n").unwrap();
        let typed = flux_mir::check_file(&hir);
        record("type", &typed.diagnostics);
        std::fs::write(root.join("phase.txt"), "mir\n").unwrap();
        let mir = flux_mir::build_file(&hir, &typed);
        record("mir", &mir.diagnostics);
        for optimized in [false, true] {
            let changed = support::inventory::changed(&originals);
            assert!(
                changed.is_empty(),
                "inputs changed before codegen: {changed:?}"
            );
            let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
            let target = if cfg!(target_os = "linux") {
                flux_codegen::Target::X86_64LinuxGnu
            } else {
                flux_codegen::Target::Aarch64AppleDarwin
            };
            let tier = if optimized {
                flux_codegen::OptimizationTier::Full
            } else {
                flux_codegen::OptimizationTier::None
            };
            let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
            let executable = root.join(format!("flux-{optimized}"));
            flux_codegen::link_executable(&object, target, &executable).unwrap();
            let actual = run(
                &mut Command::new(executable),
                &root,
                &format!("flux-run-{optimized}"),
            );
            assert_eq!(actual.status.code(), expected.status.code());
            assert_eq!(actual.stdout, expected.stdout);
            assert_eq!(actual.stderr, expected.stderr);
        }
        std::fs::write(root.join("phase.txt"), "native-matched\n").unwrap();
    }));
    let changed = support::inventory::changed(&originals);
    std::fs::write(
        root.join("source-integrity.json"),
        serde_json::to_vec_pretty(&changed).unwrap(),
    )
    .unwrap();
    assert!(changed.is_empty(), "inputs changed: {changed:?}");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
    std::fs::write(root.join("phase.txt"), "verified\n").unwrap();
}
