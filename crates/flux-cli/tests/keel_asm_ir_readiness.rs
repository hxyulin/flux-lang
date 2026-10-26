//! Unchanged pinned keel-asm-ir graph and a public assembly transport consumer.
//! Metadata is an input, never a package-manager implementation. Rustc artifacts
//! stay in the oracle directory and are never passed to Flux.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use flux_hir::{CrateCfg, SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

fn success(output: &Output, root: &Path, label: &str) {
    assert!(
        output.status.success(),
        "{label}: {}; see {}",
        String::from_utf8_lossy(&output.stderr),
        root.display()
    );
}

fn bounded(command: &mut Command, root: &Path, label: &str, seconds: u64) -> Output {
    support::process::run(command, root, label, Duration::from_secs(seconds)).output
}

fn phase(root: &Path, name: &str) {
    std::fs::write(root.join("phase.txt"), format!("{name}\n")).unwrap();
}

use support::inventory::collect as inventory;

use support::inventory::report;

#[test]
fn unchanged_keel_asm_ir_matches_rustc() {
    if std::env::var_os("FLUX_KEEL_ASM_IR_ROOT").is_some() {
        keel_asm_ir_worker();
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/keel-asm-ir-readiness/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut worker = Command::new(std::env::current_exe().unwrap());
    worker
        .args([
            "--exact",
            "unchanged_keel_asm_ir_matches_rustc",
            "--nocapture",
        ])
        .env("FLUX_KEEL_ASM_IR_ROOT", &root);
    let output = bounded(&mut worker, &root, "audit", 300);
    // Also compare after a worker panic, abort, or timeout. A killed worker
    // cannot run its own final integrity check.
    let inventory_path = root.join("source-inventory.json");
    if inventory_path.exists() {
        let originals: Vec<(PathBuf, Vec<u8>)> =
            serde_json::from_slice(&std::fs::read(inventory_path).unwrap()).unwrap();
        let changed: Vec<_> = support::inventory::changed(&originals);
        std::fs::write(
            root.join("parent-source-integrity.json"),
            serde_json::to_vec_pretty(&changed).unwrap(),
        )
        .unwrap();
        assert!(
            changed.is_empty(),
            "original source bytes changed during audit: {changed:?}"
        );
    }
    success(&output, &root, "audit");
}

fn keel_asm_ir_worker() {
    let root =
        PathBuf::from(std::env::var_os("FLUX_KEEL_ASM_IR_ROOT").expect("run the parent test"));
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut command = Command::new("cargo");
    command
        .current_dir(&repo)
        .args(["metadata", "--offline", "--locked", "--format-version", "1"]);
    let metadata = bounded(&mut command, &root, "metadata", 60);
    success(&metadata, &root, "metadata");
    std::fs::write(root.join("metadata.json"), &metadata.stdout).unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let names = ["smallvec", "keel_asm_ir", "consumer"];
    let versions = ["1.16.3", "0.1.0"];
    let dependencies: [&[usize]; 3] = [&[], &[0], &[1]];
    let mut roots = Vec::new();
    let mut editions = Vec::new();
    let mut features: Vec<Vec<String>> = Vec::new();
    let mut config = Vec::new();
    let mut originals = Vec::new();
    for index in 0..2 {
        let package = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| {
                p["name"].as_str().unwrap().replace('-', "_") == names[index]
                    && p["version"] == versions[index]
            })
            .unwrap();
        let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap());
        originals.push((manifest.clone(), std::fs::read(&manifest).unwrap()));
        let target = package["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["kind"].as_array().unwrap().iter().any(|k| k == "lib"))
            .unwrap();
        roots.push(PathBuf::from(target["src_path"].as_str().unwrap()));
        let edition = match package["edition"].as_str().unwrap() {
            "2018" => Edition::E2018,
            "2021" => Edition::E2021,
            "2024" => Edition::E2024,
            other => panic!("unreviewed source edition {other}"),
        };
        editions.push(edition);
        let node = metadata["resolve"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == package["id"])
            .unwrap();
        features.push(
            node["features"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect(),
        );
        // Check all resolved normal edges, so a new dependency cannot silently
        // disappear from this deliberately explicit source graph.
        let mut actual: Vec<_> = node["deps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| {
                d["dep_kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| k["kind"].is_null())
            })
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        let mut expected: Vec<_> = dependencies[index].iter().map(|&i| names[i]).collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
        for dep in node["deps"].as_array().unwrap().iter().filter(|d| {
            d["dep_kinds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k["kind"].is_null())
        }) {
            let index = dependencies[index]
                .iter()
                .copied()
                .find(|&i| dep["name"] == names[i])
                .unwrap();
            let expected = metadata["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| {
                    p["name"].as_str().unwrap().replace('-', "_") == names[index]
                        && p["version"] == versions[index]
                })
                .unwrap();
            assert_eq!(
                dep["pkg"], expected["id"],
                "normal edge package identity changed"
            );
        }
        if index == 1 {
            assert_eq!(
                package["source"],
                "git+https://github.com/hxyulin/keel?rev=f37f0bd48b14993e69f3db0eeb70d5babd7cd3fb#f37f0bd48b14993e69f3db0eeb70d5babd7cd3fb"
            );
            let workspace = manifest.ancestors().nth(3).unwrap().join("Cargo.toml");
            originals.push((workspace.clone(), std::fs::read(workspace).unwrap()));
        }
        config.push(serde_json::json!({"name": names[index], "version": versions[index], "root": roots[index], "manifest": manifest, "edition": package["edition"], "features": features[index], "dependencies": expected}));
        inventory(roots[index].parent().unwrap(), &mut originals);
    }
    assert_eq!(editions, [Edition::E2018, Edition::E2024]);
    assert!(
        features.iter().all(Vec::is_empty),
        "review newly enabled features"
    );
    originals.push((
        repo.join("Cargo.toml"),
        std::fs::read(repo.join("Cargo.toml")).unwrap(),
    ));
    originals.push((
        repo.join("Cargo.lock"),
        std::fs::read(repo.join("Cargo.lock")).unwrap(),
    ));
    inventory(&repo.join("library"), &mut originals);
    for path in [
        repo.join("crates/flux-hir/src/source.rs"),
        repo.join("crates/flux-cli/tests/fixtures/keel_asm_ir/consumer.rs.txt"),
    ] {
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }
    originals.sort_by(|a, b| a.0.cmp(&b.0));
    originals.dedup_by(|a, b| a.0 == b.0);
    std::fs::write(
        root.join("configuration.json"),
        serde_json::to_vec_pretty(&config).unwrap(),
    )
    .unwrap();
    // Persist the exact original bytes, not only lengths or a lossy source dump.
    std::fs::write(
        root.join("source-inventory.json"),
        serde_json::to_vec(&originals).unwrap(),
    )
    .unwrap();
    for stage in ["syntax", "hir", "type", "mir"] {
        std::fs::write(
            root.join(format!("{stage}.diagnostics")),
            "not run: waiting for preceding stages\n",
        )
        .unwrap();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::fs::write(
            root.join("consumer.rs"),
            include_str!("fixtures/keel_asm_ir/consumer.rs.txt"),
        )
        .unwrap();
        roots.push(root.join("consumer.rs"));
        editions.push(Edition::E2024);
        features.push(vec![]);
        let oracle = root.join("oracle");
        std::fs::create_dir_all(&oracle).unwrap();
        let mut libraries: Vec<PathBuf> = Vec::new();
        for index in 0..3 {
            let output = if index < 2 {
                oracle.join(format!("lib{}.rlib", names[index]))
            } else {
                oracle.join("reference")
            };
            let mut command = Command::new("rustc");
            command
                .arg(match editions[index] {
                    Edition::E2015 => "--edition=2015",
                    Edition::E2018 => "--edition=2018",
                    Edition::E2021 => "--edition=2021",
                    Edition::E2024 => "--edition=2024",
                })
                .args([
                    "-C",
                    "panic=abort",
                    "-C",
                    "overflow-checks=off",
                    "--crate-name",
                ])
                .arg(names[index])
                .arg(&roots[index])
                .arg("-L")
                .arg(&oracle)
                .arg("-o")
                .arg(&output);
            if index < 2 {
                command.arg("--crate-type=rlib");
            }
            for feature in &features[index] {
                command.arg("--cfg").arg(format!("feature=\"{feature}\""));
            }
            for &dep in dependencies[index] {
                command
                    .arg("--extern")
                    .arg(format!("{}={}", names[dep], libraries[dep].display()));
            }
            let label = format!("rustc-{}", names[index]);
            success(&bounded(&mut command, &root, &label, 60), &root, &label);
            libraries.push(output);
        }
        let expected = bounded(&mut Command::new(&libraries[2]), &root, "run-reference", 60);
        success(&expected, &root, "run-reference");
        let mut db = SourceDb::new();
        let sources: Vec<_> = roots
            .iter()
            .enumerate()
            .map(|(index, source)| SourceCrate {
                root: db.load_with_edition(source, editions[index]).unwrap(),
                name: Symbol::intern(names[index]),
                dependencies: dependencies[index]
                    .iter()
                    .map(|&dep| (Symbol::intern(names[dep]), dep))
                    .collect(),
            })
            .collect();
        for (index, source) in sources.iter().enumerate() {
            db.set_crate_cfg(
                source.root,
                CrateCfg {
                    names: vec![],
                    values: features[index]
                        .iter()
                        .map(|feature| (Symbol::intern("feature"), Symbol::intern(feature)))
                        .collect(),
                },
            )
            .unwrap();
        }
        phase(&root, "syntax-hir");
        let hir = flux_hir::lower_sources(&mut db, &sources);
        report(&root, "syntax", &db, &db.syntax_errors());
        report(&root, "hir", &db, &hir.diagnostics);
        std::fs::write(
            root.join("loaded-files.json"),
            serde_json::to_vec_pretty(&db.files().map(|(_, f)| &f.path).collect::<Vec<_>>())
                .unwrap(),
        )
        .unwrap();
        let clean_hir = db.syntax_errors().is_empty() && hir.diagnostics.is_empty();
        assert!(clean_hir, "syntax/HIR blockers; see {}", root.display());
        phase(&root, "type");
        let typed = flux_mir::check_file(&hir);
        report(&root, "type", &db, &typed.diagnostics);
        assert!(
            typed.diagnostics.is_empty(),
            "type blockers; see {}",
            root.display()
        );
        assert!(flux_ty::check_main(&hir.items, &typed.sigs).is_none());
        phase(&root, "mir");
        let mir = flux_mir::build_file(&hir, &typed);
        report(&root, "mir", &db, &mir.diagnostics);
        assert!(
            mir.diagnostics.is_empty(),
            "MIR blockers; see {}",
            root.display()
        );
        for (label, tier) in [
            ("unoptimized", flux_codegen::OptimizationTier::None),
            ("optimized", flux_codegen::OptimizationTier::Full),
        ] {
            integrity(&root, &originals, "pre-codegen-source-integrity");
            phase(&root, &format!("codegen-{label}"));
            let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
            let target = flux_codegen::Target::Aarch64AppleDarwin;
            phase(&root, &format!("compile-object-{label}"));
            let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
            let executable = root.join(format!("flux-{label}"));
            phase(&root, &format!("link-{label}"));
            flux_codegen::link_executable(&object, target, &executable).unwrap();
            phase(&root, &format!("execute-{label}"));
            let actual = bounded(
                &mut Command::new(executable),
                &root,
                &format!("run-flux-{label}"),
                60,
            );
            assert_eq!(actual.status.code(), expected.status.code());
            assert_eq!(actual.stdout, expected.stdout);
            assert_eq!(actual.stderr, expected.stderr);
        }
        phase(&root, "native-verified");
    }));
    integrity(&root, &originals, "source-integrity");
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

fn integrity(root: &Path, originals: &[(PathBuf, Vec<u8>)], label: &str) {
    let changed: Vec<_> = support::inventory::changed(originals);
    std::fs::write(
        root.join(format!("{label}.json")),
        serde_json::to_vec_pretty(&changed).unwrap(),
    )
    .unwrap();
    assert!(
        changed.is_empty(),
        "original source bytes changed during audit: {changed:?}"
    );
}
