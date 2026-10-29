//! Unchanged flux-mir source graph and a public build/evaluation consumer.
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

// The unoptimized consumer rechecks the entire embedded library for each
// input. The flux-built executables took 75-87 s beside other tests and over
// 120 s on a loaded machine, where the 31 s reference took 87 s. Allow that
// contention without changing the compiler's evaluation or recursion limits.
const NATIVE_RUN_SECONDS: u64 = 600;

mod cases {
    include!("fixtures/mir_selfhost/cases.rs.txt");
}

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
fn unchanged_full_mir_matches_rustc() {
    if std::env::var_os("FLUX_MIR_SELFHOST_ROOT").is_some() {
        mir_selfhost_worker();
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch = support::scratch::Scratch::new(repo.join("out/mir-selfhost"));
    let root = scratch.path().to_path_buf();
    let mut worker = Command::new(std::env::current_exe().unwrap());
    worker
        .args(["--exact", "unchanged_full_mir_matches_rustc", "--nocapture"])
        .env("FLUX_MIR_SELFHOST_ROOT", &root);
    let output = bounded(&mut worker, &root, "audit", 1200);
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

fn mir_selfhost_worker() {
    let root =
        PathBuf::from(std::env::var_os("FLUX_MIR_SELFHOST_ROOT").expect("run the parent test"));
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
    let names = [
        "flux_arena",
        "hashbrown",
        "rustc_hash",
        "flux_intern",
        "memchr",
        "unicode_ident",
        "flux_syntax",
        "flux_hir",
        "flux_ty_ir",
        "flux_solve",
        "flux_ty",
        "smallvec",
        "flux_mir",
        "consumer",
    ];
    let versions = [
        "0.1.0", "0.17.1", "2.1.3", "0.1.0", "2.8.3", "1.0.27", "0.1.0", "0.1.0", "0.1.0", "0.1.0",
        "0.1.0", "1.16.3", "0.1.0",
    ];
    let dependencies: [&[usize]; 14] = [
        &[],
        &[],
        &[],
        &[0, 1, 2],
        &[],
        &[],
        &[4, 5],
        &[0, 2, 3, 6],
        &[0, 1, 2, 3, 6, 7],
        &[0, 2, 3, 7, 8],
        &[0, 2, 3, 6, 7, 8, 9],
        &[],
        &[0, 2, 3, 6, 7, 9, 10, 11],
        &[3, 7, 10, 12],
    ];
    let mut roots = Vec::new();
    let mut editions = Vec::new();
    let mut features: Vec<Vec<String>> = Vec::new();
    let mut config = Vec::new();
    let mut originals = Vec::new();
    for index in 0..13 {
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
        config.push(serde_json::json!({"name": names[index], "version": versions[index], "root": roots[index], "manifest": manifest, "edition": package["edition"], "features": features[index], "dependencies": expected}));
        inventory(roots[index].parent().unwrap(), &mut originals);
    }
    assert_eq!(editions[11], Edition::E2018);
    assert!(
        features[11].is_empty(),
        "review newly enabled smallvec features"
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
            include_str!("fixtures/mir_selfhost/consumer.rs.txt"),
        )
        .unwrap();
        std::fs::write(
            root.join("cases.rs"),
            include_str!("fixtures/mir_selfhost/cases.rs.txt"),
        )
        .unwrap();
        roots.push(root.join("consumer.rs"));
        editions.push(Edition::E2024);
        features.push(vec![]);
        let oracle = root.join("oracle");
        std::fs::create_dir_all(&oracle).unwrap();
        let mut libraries: Vec<PathBuf> = Vec::new();
        for index in 0..14 {
            let output = if index < 13 {
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
            if index < 13 {
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
        for &(name, source, _, accepts) in cases::CASES {
            let input = oracle.join(format!("{name}.rs"));
            std::fs::write(&input, source).unwrap();
            let mut command = Command::new("rustc");
            command
                .args(["--edition=2024", "--emit=metadata"])
                .arg(&input)
                .arg("-o")
                .arg(oracle.join(format!("{name}.rmeta")));
            let output = bounded(&mut command, &root, &format!("rustc-case-{name}"), 60);
            assert!(
                output.status.code().is_some(),
                "rustc case {name} crashed or timed out"
            );
            assert_eq!(
                output.status.success(),
                accepts,
                "rustc case {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        // The consumers only compute and print, so the reference and both
        // flux-built executables run while other work continues.
        std::thread::scope(|scope| {
            let reference = scope.spawn(|| {
                bounded(
                    &mut Command::new(&libraries[13]),
                    &root,
                    "run-reference",
                    NATIVE_RUN_SECONDS,
                )
            });
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
            // Explicit opt-in permits read-only inventories below known admission
            // errors. Reports retain their provisional names and never reach codegen.
            let provisional = std::env::var_os("FLUX_MIR_PROVISIONAL").is_some()
                && !clean_hir
                && db.syntax_errors().is_empty()
                && hir.diagnostics.iter().all(|d| {
                    d.message.starts_with("cannot find")
                        || d.message.starts_with("unresolved import `std::")
                        || d.message == "impl Trait in edition 2021 are not supported yet"
                        || d.message == "higher-ranked bounds are not supported yet"
                });
            if !clean_hir && !provisional {
                panic!("syntax/HIR blockers; see {}", root.display());
            }
            phase(&root, "type");
            let typed = flux_mir::check_file(&hir);
            report(
                &root,
                if provisional {
                    "provisional-type"
                } else {
                    "type"
                },
                &db,
                &typed.diagnostics,
            );
            if !clean_hir || !typed.diagnostics.is_empty() {
                if std::env::var_os("FLUX_MIR_PROVISIONAL").is_some() {
                    let mut errors = hir.diagnostics.clone();
                    errors.extend(typed.diagnostics.iter().cloned());
                    phase(&root, "provisional-mir");
                    let diagnostics = flux_mir::check_clean_bodies(&hir, &typed, &errors);
                    report(&root, "provisional-mir", &db, &diagnostics);
                }
                panic!("type/admission blockers; see {}", root.display());
            }
            assert!(flux_ty::check_main(&hir.items, &typed.sigs).is_none());
            phase(&root, "mir");
            let mir = flux_mir::build_file(&hir, &typed);
            report(&root, "mir", &db, &mir.diagnostics);
            assert!(
                mir.diagnostics.is_empty(),
                "MIR blockers; see {}",
                root.display()
            );
            let mut runs = Vec::new();
            for (label, tier) in [
                ("unoptimized", flux_codegen::OptimizationTier::None),
                ("optimized", flux_codegen::OptimizationTier::Full),
            ] {
                phase(&root, &format!("codegen-{label}"));
                let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
                let target = flux_codegen::Target::Aarch64AppleDarwin;
                phase(&root, &format!("compile-object-{label}"));
                let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
                let executable = root.join(format!("flux-{label}"));
                phase(&root, &format!("link-{label}"));
                flux_codegen::link_executable(&object, target, &executable).unwrap();
                let root = &root;
                runs.push((
                    label,
                    scope.spawn(move || {
                        bounded(
                            &mut Command::new(executable),
                            root,
                            &format!("run-flux-{label}"),
                            NATIVE_RUN_SECONDS,
                        )
                    }),
                ));
            }
            phase(&root, "execute");
            let expected = reference.join().unwrap();
            success(&expected, &root, "run-reference");
            for (label, run) in runs {
                let actual = run.join().unwrap();
                assert_eq!(actual.status.code(), expected.status.code(), "{label}");
                assert_eq!(actual.stdout, expected.stdout, "{label}");
                assert_eq!(actual.stderr, expected.stderr, "{label}");
            }
            phase(&root, "native-verified");
        });
    }));
    let changed: Vec<_> = support::inventory::changed(&originals);
    std::fs::write(
        root.join("source-integrity.json"),
        serde_json::to_vec_pretty(&changed).unwrap(),
    )
    .unwrap();
    assert!(
        changed.is_empty(),
        "original source bytes changed during audit: {changed:?}"
    );
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
