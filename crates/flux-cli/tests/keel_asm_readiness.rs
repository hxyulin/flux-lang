//! Unchanged pinned keel-asm normal closure and a native assembler consumer.
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
#[ignore = "readiness audit requires MIR lifetime proofs, const drop and borrow fixes; run explicitly with --ignored"]
fn unchanged_keel_asm_matches_rustc() {
    if std::env::var_os("FLUX_KEEL_ASM_ROOT").is_some() {
        keel_asm_worker();
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/keel-asm-readiness/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut worker = Command::new(std::env::current_exe().unwrap());
    worker
        .args([
            "--ignored",
            "--exact",
            "unchanged_keel_asm_matches_rustc",
            "--nocapture",
        ])
        .env("FLUX_KEEL_ASM_ROOT", &root);
    let output = bounded(&mut worker, &root, "audit", 300);
    // Also compare after a worker panic, abort, or timeout. A killed worker
    // cannot run its own final integrity check.
    let inventory_path = root.join("source-inventory.json");
    if inventory_path.exists() {
        let originals: Vec<(PathBuf, Vec<u8>)> =
            serde_json::from_slice(&std::fs::read(&inventory_path).unwrap()).unwrap();
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
    assert!(
        inventory_path.is_file(),
        "audit worker did not produce a source inventory; see {}",
        root.display()
    );
}

fn keel_asm_worker() {
    let root = PathBuf::from(std::env::var_os("FLUX_KEEL_ASM_ROOT").expect("run the parent test"));
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
    // Topological order is explicit. Duplicate crate names retain package IDs,
    // dependency indices, rustc metadata, and distinct artifact paths.
    let names = [
        "bumpalo",
        "smallvec",
        "keel_asm_ir",
        "memchr",
        "cfg_if",
        "crc32fast",
        "foldhash",
        "hashbrown",
        "equivalent",
        "hashbrown",
        "indexmap",
        "object",
        "keel_asm",
        "consumer",
    ];
    let versions = [
        "3.20.3", "1.16.3", "0.1.0", "2.8.3", "1.0.5", "1.5.3", "0.1.5", "0.15.5", "1.0.3",
        "0.17.1", "2.14.2", "0.36.7", "0.1.0",
    ];
    let edition_names = [
        "2021", "2018", "2024", "2021", "2018", "2021", "2021", "2021", "2015", "2024", "2024",
        "2018", "2024",
    ];
    let expected_features: [&[&str]; 13] = [
        &["collections", "default"],
        &[],
        &[],
        &["alloc", "default", "std"],
        &[],
        &["std"],
        &[],
        &["default-hasher"],
        &[],
        &[],
        &["default", "std"],
        &[
            "archive",
            "coff",
            "elf",
            "macho",
            "pe",
            "read",
            "read_core",
            "std",
            "unaligned",
            "write",
            "write_core",
            "write_std",
            "xcoff",
        ],
        &[],
    ];
    let dependencies: [&[usize]; 14] = [
        &[],
        &[],
        &[1],
        &[],
        &[],
        &[4],
        &[],
        &[6],
        &[],
        &[],
        &[8, 9],
        &[5, 7, 10, 3],
        &[0, 2, 3, 11, 1],
        &[12, 11],
    ];
    let packages: Vec<_> = (0..13)
        .map(|index| {
            metadata["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| {
                    p["name"].as_str().unwrap().replace('-', "_") == names[index]
                        && p["version"] == versions[index]
                })
                .expect("review changed dependency closure")
        })
        .collect();
    let mut roots = Vec::new();
    let mut editions = Vec::new();
    let mut features: Vec<Vec<String>> = Vec::new();
    let mut cfgs: Vec<Vec<String>> = vec![vec![]; 14];
    let mut config = Vec::new();
    let mut originals = Vec::new();
    let mut scripts = Vec::new();
    for index in 0..13 {
        let package = packages[index];
        let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap());
        originals.push((manifest.clone(), std::fs::read(&manifest).unwrap()));
        let target = package["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["kind"].as_array().unwrap().iter().any(|k| k == "lib"))
            .unwrap();
        roots.push(PathBuf::from(target["src_path"].as_str().unwrap()));
        assert_eq!(package["edition"], edition_names[index]);
        editions.push(match edition_names[index] {
            "2015" => Edition::E2015,
            "2018" => Edition::E2018,
            "2021" => Edition::E2021,
            "2024" => Edition::E2024,
            _ => unreachable!(),
        });
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
        assert_eq!(
            features[index], expected_features[index],
            "review changed features for {}",
            package["id"]
        );
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
            .map(|d| (d["name"].as_str().unwrap(), d["pkg"].as_str().unwrap()))
            .collect();
        let mut expected: Vec<_> = dependencies[index]
            .iter()
            .map(|&i| (names[i], packages[i]["id"].as_str().unwrap()))
            .collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "normal edges for {}", package["id"]);
        if index == 2 || index == 12 {
            assert_eq!(
                package["source"],
                "git+https://github.com/hxyulin/keel?rev=f37f0bd48b14993e69f3db0eeb70d5babd7cd3fb#f37f0bd48b14993e69f3db0eeb70d5babd7cd3fb"
            );
            let workspace = manifest.ancestors().nth(3).unwrap().join("Cargo.toml");
            originals.push((workspace.clone(), std::fs::read(workspace).unwrap()));
        }
        // Include original build scripts, including any helper modules outside src.
        inventory(manifest.parent().unwrap(), &mut originals);
        for target in package["targets"].as_array().unwrap().iter().filter(|t| {
            t["kind"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k == "custom-build")
        }) {
            assert!(
                index == 5 || index == 11,
                "review new build script for {}",
                package["id"]
            );
            scripts.push((index, PathBuf::from(target["src_path"].as_str().unwrap())));
        }
        config.push(serde_json::json!({"index": index, "id": package["id"], "name": names[index], "version": versions[index], "root": roots[index], "manifest": manifest, "edition": package["edition"], "features": features[index], "dependencies": expected, "dependency_indices": dependencies[index], "rustc_metadata": format!("keel_asm_readiness_{index}"), "rustc_output": format!("oracle/lib{}_{index}.rlib", names[index])}));
    }
    assert_eq!(scripts.len(), 2);
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
        repo.join("crates/flux-cli/tests/keel_asm_readiness.rs"),
        repo.join("crates/flux-cli/tests/fixtures/keel_asm/consumer.rs.txt"),
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
    phase(&root, "inputs-audited");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        integrity(&root, &originals, "pre-reference-source-integrity");
        std::fs::write(
            root.join("consumer.rs"),
            include_str!("fixtures/keel_asm/consumer.rs.txt"),
        )
        .unwrap();
        roots.push(root.join("consumer.rs"));
        editions.push(Edition::E2024);
        features.push(vec![]);
        phase(&root, "build-scripts");
        let version = bounded(
            Command::new("rustc").args(["--version", "--verbose"]),
            &root,
            "rustc-version",
            10,
        );
        success(&version, &root, "rustc-version");
        let version_text = String::from_utf8(version.stdout).unwrap();
        let minor: u32 = version_text
            .lines()
            .find_map(|line| line.strip_prefix("release: 1."))
            .unwrap()
            .split('.')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        for (index, script) in &scripts {
            let label = format!("build-script-{index}-{}", names[*index]);
            let executable = root.join(&label);
            success(
                &bounded(
                    Command::new("rustc")
                        .arg(script)
                        .arg(format!("--edition={}", edition_names[*index]))
                        .arg("-o")
                        .arg(&executable),
                    &root,
                    &format!("compile-{label}"),
                    60,
                ),
                &root,
                &label,
            );
            let output = bounded(
                Command::new(executable)
                    .current_dir(script.parent().unwrap())
                    .env("RUSTC", "rustc"),
                &root,
                &label,
                30,
            );
            success(&output, &root, &label);
            cfgs[*index] = String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .filter_map(|line| line.strip_prefix("cargo:rustc-cfg=").map(str::to_owned))
                .collect();
            let expected: Vec<&str> = if *index == 11 {
                if minor >= 81 {
                    vec!["core_error"]
                } else {
                    vec![]
                }
            } else {
                let mut cfg = Vec::new();
                if minor >= 80 {
                    cfg.push("stable_arm_crc32_intrinsics");
                }
                if minor >= 89 {
                    cfg.push("stable_vpclmulqdq");
                }
                cfg
            };
            assert_eq!(cfgs[*index], expected);
        }
        std::fs::write(
            root.join("build-script-cfg.json"),
            serde_json::to_vec_pretty(&cfgs).unwrap(),
        )
        .unwrap();
        let oracle = root.join("oracle");
        std::fs::create_dir_all(&oracle).unwrap();
        let mut libraries: Vec<PathBuf> = Vec::new();
        phase(&root, "rustc-reference");
        for index in 0..14 {
            let output = if index < 13 {
                oracle.join(format!("lib{}_{index}.rlib", names[index]))
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
            if index < 13 {
                command.arg("--crate-type=rlib");
            }
            command.args(["-C", &format!("metadata=keel_asm_readiness_{index}")]);
            for cfg in &cfgs[index] {
                command.arg("--cfg").arg(cfg);
            }
            for feature in &features[index] {
                command.arg("--cfg").arg(format!("feature=\"{feature}\""));
            }
            for &dep in dependencies[index] {
                command
                    .arg("--extern")
                    .arg(format!("{}={}", names[dep], libraries[dep].display()));
            }
            let label = format!("rustc-{index}-{}", names[index]);
            success(&bounded(&mut command, &root, &label, 60), &root, &label);
            libraries.push(output);
        }
        let expected = bounded(
            &mut Command::new(&libraries[13]),
            &root,
            "run-reference",
            60,
        );
        success(&expected, &root, "run-reference");
        integrity(&root, &originals, "pre-flux-source-integrity");
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
                    names: cfgs[index]
                        .iter()
                        .map(|name| Symbol::intern(name))
                        .collect(),
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
