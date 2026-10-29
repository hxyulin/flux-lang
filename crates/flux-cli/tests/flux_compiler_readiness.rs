//! Audits the unchanged complete Flux CLI source and target dependency graph.
//! Cargo supplies the reference artifacts and real build outputs, never Flux code.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use flux_hir::{CrateCfg, SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use support::inventory::{Files, collect, report};

const TEST: &str = "unchanged_flux_compiler_readiness";
const SPEC: &str = include_str!("fixtures/flux_compiler/graph.json");

fn run(command: &mut Command, root: &Path, label: &str, seconds: u64) -> Output {
    support::process::run(command, root, label, Duration::from_secs(seconds)).output
}
fn success(output: &Output, root: &Path, label: &str) {
    assert!(
        output.status.success(),
        "{label}: {}; see {}",
        String::from_utf8_lossy(&output.stderr),
        root.display()
    );
}
fn save(root: &Path, name: &str, value: &impl serde::Serialize) {
    std::fs::write(root.join(name), serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn phase(root: &Path, name: &str) {
    std::fs::write(root.join("phase.txt"), name).unwrap();
}
fn integrity(root: &Path, files: &Files, name: &str) {
    let changed = support::inventory::changed(files);
    save(root, name, &changed);
    assert!(changed.is_empty(), "input bytes changed: {changed:?}");
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
#[ignore = "full Flux compiler source/input audit; run explicitly with --ignored --nocapture"]
fn unchanged_flux_compiler_readiness() {
    if let Some(root) = std::env::var_os("FLUX_COMPILER_ROOT") {
        let root = PathBuf::from(root);
        if std::env::var_os("FLUX_COMPILER_FLUX").is_some() {
            flux_compiler(&root);
        } else {
            audit(&root);
        }
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!(
        "out/flux-compiler-readiness/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let output = run(
        Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", TEST, "--nocapture"])
            .env("FLUX_COMPILER_ROOT", &root),
        &root,
        "audit",
        1200,
    );
    let inventory = root.join("source-inventory.json");
    if inventory.exists() {
        let files: Files = serde_json::from_slice(&std::fs::read(inventory).unwrap()).unwrap();
        integrity(&root, &files, "parent-source-integrity.json");
    }
    success(&output, &root, "full compiler audit");
}

fn audit(root: &Path) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let spec: Value = serde_json::from_str(SPEC).unwrap();
    let local_keel = std::env::var_os("FLUX_COMPILER_KEEL_CHECKOUT")
        .map(|path| PathBuf::from(path).canonicalize().unwrap());
    if let Some(checkout) = &local_keel {
        let output = run(
            Command::new("git")
                .current_dir(checkout)
                .args(["rev-parse", "HEAD"]),
            root,
            "local-keel-base",
            10,
        );
        success(&output, root, "local Keel base");
        let source = spec["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "keel-codegen")
            .unwrap()["source"]
            .as_str()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            source.rsplit_once('#').unwrap().1,
            "local Keel must be based on the reviewed pin"
        );
        let diff = run(
            Command::new("git")
                .current_dir(checkout)
                .args(["diff", "--binary", "HEAD"]),
            root,
            "local-keel-patch",
            10,
        );
        success(&diff, root, "local Keel patch");
        std::fs::write(root.join("local-keel.patch"), &diff.stdout).unwrap();
        save(root, "local-keel-checkout.json", checkout);
    }
    let output = run(
        Command::new("cargo").current_dir(&repo).args([
            "metadata",
            "--offline",
            "--locked",
            "--filter-platform",
            "aarch64-apple-darwin",
            "--format-version",
            "1",
        ]),
        root,
        "metadata",
        60,
    );
    success(&output, root, "metadata");
    std::fs::write(root.join("metadata.json"), &output.stdout).unwrap();
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut packages = Vec::new();
    let mut originals = Vec::new();
    for expected in spec["packages"].as_array().unwrap() {
        let package = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == expected["name"] && p["version"] == expected["version"])
            .expect("review changed backend package");
        if let Some(checkout) = local_keel
            .as_ref()
            .filter(|_| package["name"].as_str().unwrap().starts_with("keel-"))
        {
            assert!(package["source"].is_null(), "expected a local Keel package");
            assert_eq!(
                Path::new(package["manifest_path"].as_str().unwrap())
                    .canonicalize()
                    .unwrap(),
                checkout
                    .join("crates")
                    .join(package["name"].as_str().unwrap())
                    .join("Cargo.toml")
                    .canonicalize()
                    .unwrap(),
                "local Keel package identity changed"
            );
        } else {
            assert_eq!(package["source"], expected["source"]);
        }
        assert_eq!(package["edition"], expected["edition"]);
        let node = metadata["resolve"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == package["id"])
            .unwrap();
        assert_eq!(node["features"], expected["features"]);
        let targets = package["targets"].as_array().unwrap();
        let library = targets
            .iter()
            .find(|t| {
                t["kind"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| k == "lib" || k == "proc-macro")
            })
            .unwrap();
        assert_eq!(
            library["kind"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k == "proc-macro"),
            expected["proc_macro"] == true
        );
        let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap());
        collect(manifest.parent().unwrap(), &mut originals);
        originals.push((manifest.clone(), std::fs::read(&manifest).unwrap()));
        if package["name"].as_str().unwrap().starts_with("keel-") {
            let workspace = manifest.ancestors().nth(3).unwrap().join("Cargo.toml");
            originals.push((workspace.clone(), std::fs::read(workspace).unwrap()));
        }
        let version = package["version"].as_str().unwrap();
        let parts: Vec<_> = version.split(['.', '-', '+']).collect();
        let mut environment = BTreeMap::from([
            (
                "CARGO_MANIFEST_DIR".to_owned(),
                manifest.parent().unwrap().to_string_lossy().into_owned(),
            ),
            (
                "CARGO_PKG_NAME".into(),
                package["name"].as_str().unwrap().into(),
            ),
            (
                "CARGO_CRATE_NAME".into(),
                library["name"].as_str().unwrap().into(),
            ),
            ("CARGO_PKG_VERSION".into(), version.into()),
            ("CARGO_PKG_VERSION_MAJOR".into(), parts[0].into()),
            ("CARGO_PKG_VERSION_MINOR".into(), parts[1].into()),
            ("CARGO_PKG_VERSION_PATCH".into(), parts[2].into()),
            (
                "CARGO_PKG_VERSION_PRE".into(),
                version
                    .split_once('-')
                    .map(|(_, p)| p.split('+').next().unwrap())
                    .unwrap_or("")
                    .into(),
            ),
            (
                "CARGO_PKG_AUTHORS".into(),
                strings(&package["authors"]).join(":"),
            ),
        ]);
        for (key, field) in [
            ("CARGO_PKG_DESCRIPTION", "description"),
            ("CARGO_PKG_HOMEPAGE", "homepage"),
            ("CARGO_PKG_REPOSITORY", "repository"),
            ("CARGO_PKG_LICENSE", "license"),
        ] {
            environment.insert(key.into(), package[field].as_str().unwrap_or("").into());
        }
        packages.push(serde_json::json!({"package":package["name"],"version":package["version"],"manifest":manifest,"id":package["id"],"name":library["name"],"root":library["src_path"],"edition":package["edition"],"features":expected["features"],"proc_macro":expected["proc_macro"],"dependencies":expected["dependencies"],"environment":environment}));
    }
    for (index, package) in packages.iter().enumerate() {
        let node = metadata["resolve"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == package["id"])
            .unwrap();
        let actual: BTreeSet<_> = node["deps"]
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
            .map(|d| {
                (
                    d["name"].as_str().unwrap().to_owned(),
                    d["pkg"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let expected: BTreeSet<_> = package["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                (
                    d["name"].as_str().unwrap().to_owned(),
                    packages[d["index"].as_u64().unwrap() as usize]["id"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                )
            })
            .collect();
        assert_eq!(actual, expected, "normal edges for package {index}");
    }
    for path in [
        repo.join("Cargo.toml"),
        repo.join("Cargo.lock"),
        repo.join("crates/flux-hir/src/source.rs"),
        repo.join("crates/flux-cli/tests/flux_compiler_readiness.rs"),
        repo.join("crates/flux-cli/tests/fixtures/flux_compiler/smoke.rs.txt"),
        repo.join("crates/flux-cli/tests/fixtures/flux_compiler/graph.json"),
    ] {
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }
    collect(&repo.join("library"), &mut originals);
    if local_keel.is_some() {
        let config = repo.join(".cargo/config.toml");
        originals.push((config.clone(), std::fs::read(config).unwrap()));
    }
    let codegen = packages
        .iter()
        .find(|p| p["package"] == "keel-codegen")
        .unwrap();
    let patterns = Path::new(codegen["root"].as_str().unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("patterns.txt");
    originals.push((patterns.clone(), std::fs::read(patterns).unwrap()));
    save(root, "configuration.json", &packages);
    originals.sort_by(|a, b| a.0.cmp(&b.0));
    originals.dedup_by(|a, b| a.0 == b.0);
    std::fs::write(
        root.join("source-inventory.json"),
        serde_json::to_vec(&originals).unwrap(),
    )
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        integrity(root, &originals, "pre-reference-source-integrity.json");
        phase(root, "cargo-reference-inputs");
        let output = run(
            Command::new("cargo").current_dir(&repo).args([
                "build",
                "--workspace",
                "--offline",
                "--locked",
                "--message-format=json",
            ]),
            root,
            "cargo-reference",
            180,
        );
        success(&output, root, "Cargo reference build");
        let messages: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        save(root, "cargo-messages.json", &messages);
        let mut libraries = BTreeMap::new();
        let mut host_macros = BTreeMap::new();
        let mut scripts = BTreeMap::new();
        for message in &messages {
            if message["reason"] == "compiler-artifact" {
                if message["target"]["kind"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| k == "proc-macro")
                {
                    let artifact = message["filenames"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find_map(|p| p.as_str().filter(|p| p.ends_with(".dylib")))
                        .unwrap();
                    host_macros.insert(
                        message["package_id"].as_str().unwrap().to_owned(),
                        serde_json::json!({"artifact":artifact,"features":message["features"]}),
                    );
                }

                if let Some(file) = message["filenames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p.as_str().unwrap().ends_with(".rlib"))
                {
                    libraries.insert(
                        message["package_id"].as_str().unwrap().to_owned(),
                        file.as_str().unwrap().to_owned(),
                    );
                }
            } else if message["reason"] == "build-script-executed" {
                scripts.insert(
                    message["package_id"].as_str().unwrap().to_owned(),
                    message.clone(),
                );
            }
        }
        for p in &packages {
            if p["proc_macro"] == true {
                let file = PathBuf::from(
                    host_macros[p["id"].as_str().unwrap()]["artifact"]
                        .as_str()
                        .unwrap(),
                );
                originals.push((file.clone(), std::fs::read(file).unwrap()));
            }
        }
        collect(&repo.join("crates/flux-cli/src/proc_macro"), &mut originals);
        save(root, "host-macros.json", &host_macros);
        let version = run(
            Command::new("rustc").arg("-vV"),
            root,
            "host-rustc-version",
            30,
        );
        success(&version, root, "host rustc version");
        std::fs::write(root.join("host-rustc-version.txt"), &version.stdout).unwrap();
        for package in &packages {
            if let Some(script) = scripts.get(package["id"].as_str().unwrap()) {
                collect(
                    Path::new(script["out_dir"].as_str().unwrap()),
                    &mut originals,
                );
            }
        }
        originals.sort_by(|a, b| a.0.cmp(&b.0));
        originals.dedup_by(|a, b| a.0 == b.0);
        std::fs::write(
            root.join("source-inventory.json"),
            serde_json::to_vec(&originals).unwrap(),
        )
        .unwrap();
        save(root, "build-outputs.json", &scripts);
        let executable = messages
            .iter()
            .find_map(|m| {
                (m["reason"] == "compiler-artifact" && m["target"]["name"] == "flux")
                    .then(|| m["executable"].as_str())
                    .flatten()
            })
            .expect("Cargo built the actual Flux executable");
        std::fs::copy(executable, root.join("reference-flux")).unwrap();
        phase(root, "reference-cli");
        cli_contract(&root.join("reference-flux"), root, "reference");
        integrity(root, &originals, "pre-flux-source-integrity.json");
        std::fs::write(root.join("input.diagnostics"), "count=0\n").unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--ignored", "--exact", TEST, "--nocapture"])
            .env("FLUX_COMPILER_ROOT", root)
            .env("FLUX_COMPILER_FLUX", "1");
        success(
            &run(&mut command, root, "flux-audit", 1080),
            root,
            "full compiler graph",
        );
        phase(root, "native-verified");
    }));
    integrity(root, &originals, "source-integrity.json");
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

fn flux_compiler(root: &Path) {
    let label = "compiler";
    let packages: Vec<Value> =
        serde_json::from_slice(&std::fs::read(root.join("configuration.json")).unwrap()).unwrap();
    let scripts: BTreeMap<String, Value> =
        serde_json::from_slice(&std::fs::read(root.join("build-outputs.json")).unwrap()).unwrap();
    let spec: Value = serde_json::from_str(SPEC).unwrap();
    let mut selected = BTreeSet::new();
    select_packages(
        &packages,
        spec["root_index"].as_u64().unwrap() as usize,
        &mut selected,
    );
    let indices: BTreeMap<usize, usize> = selected
        .iter()
        .enumerate()
        .map(|(new, &old)| (old, new))
        .collect();
    save(root, "target-indices.json", &selected);
    for stage in ["syntax", "hir", "type", "mir"] {
        std::fs::write(
            root.join(format!("{label}-{stage}.diagnostics")),
            "not run: waiting for preceding stages\n",
        )
        .unwrap();
    }
    let host_macros: BTreeMap<String, Value> =
        serde_json::from_slice(&std::fs::read(root.join("host-macros.json")).unwrap()).unwrap();
    let version = std::fs::read_to_string(root.join("host-rustc-version.txt")).unwrap();
    let mut db = SourceDb::new();
    let mut sources = Vec::new();
    for &i in &selected {
        let p = &packages[i];
        let edition = match p["edition"].as_str().unwrap() {
            "2015" => Edition::E2015,
            "2018" => Edition::E2018,
            "2021" => Edition::E2021,
            "2024" => Edition::E2024,
            _ => panic!("unsupported source edition"),
        };
        let source_root = if p["proc_macro"] == true {
            let host = &host_macros[p["id"].as_str().unwrap()];
            let mut cfg = CrateCfg::default();
            cfg.names.push(Symbol::intern("proc_macro"));
            cfg.values = strings(&host["features"])
                .iter()
                .map(|f| (Symbol::intern("feature"), Symbol::intern(f)))
                .collect();
            if let Some(script) = scripts.get(p["id"].as_str().unwrap()) {
                for c in strings(&script["cfgs"]) {
                    if let Some((k, v)) = c.split_once('=') {
                        cfg.values
                            .push((Symbol::intern(k), Symbol::intern(v.trim_matches('"'))));
                    } else {
                        cfg.names.push(Symbol::intern(&c));
                    }
                }
            }
            let artifact = Path::new(host["artifact"].as_str().unwrap());
            let exports = flux_cli::proc_macro::exports(
                &version,
                artifact,
                Path::new(p["root"].as_str().unwrap()),
                cfg,
            )
            .unwrap();
            db.add_proc_macro_crate(artifact.to_owned(), exports)
        } else {
            db.load_with_edition(Path::new(p["root"].as_str().unwrap()), edition)
                .unwrap()
        };
        let source = SourceCrate {
            root: source_root,
            name: Symbol::intern(p["name"].as_str().unwrap()),
            dependencies: p["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|_| p["proc_macro"] != true)
                .map(|d| {
                    (
                        Symbol::intern(d["name"].as_str().unwrap()),
                        indices[&(d["index"].as_u64().unwrap() as usize)],
                    )
                })
                .collect(),
        };
        let mut cfg = CrateCfg {
            names: Vec::new(),
            values: strings(&p["features"])
                .into_iter()
                .map(|f| (Symbol::intern("feature"), Symbol::intern(&f)))
                .collect(),
        };
        let mut environment: BTreeMap<String, String> = p["environment"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        if let Some(script) = scripts.get(p["id"].as_str().unwrap()) {
            environment.insert("OUT_DIR".into(), script["out_dir"].as_str().unwrap().into());
            for entry in script["env"].as_array().unwrap() {
                environment.insert(
                    entry[0].as_str().unwrap().into(),
                    entry[1].as_str().unwrap().into(),
                );
            }
            for text in strings(&script["cfgs"]) {
                if let Some((key, value)) = text.split_once('=') {
                    cfg.values
                        .push((Symbol::intern(key), Symbol::intern(value.trim_matches('"'))));
                } else {
                    cfg.names.push(Symbol::intern(&text));
                }
            }
        }
        db.set_crate_env(source.root, environment).unwrap();
        db.set_crate_cfg(source.root, cfg).unwrap();
        sources.push(source);
    }
    let cli_index = packages
        .iter()
        .position(|p| p["package"] == "flux-cli")
        .unwrap();
    let cli = &packages[cli_index];
    let binary = db
        .load_with_edition(
            &Path::new(cli["root"].as_str().unwrap()).with_file_name("main.rs"),
            Edition::E2024,
        )
        .unwrap();
    let mut binary_env: BTreeMap<String, String> = cli["environment"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect();
    binary_env.insert("CARGO_CRATE_NAME".into(), "flux".into());
    db.set_crate_env(binary, binary_env).unwrap();
    db.set_crate_cfg(
        binary,
        CrateCfg {
            names: Vec::new(),
            values: strings(&cli["features"])
                .iter()
                .map(|f| (Symbol::intern("feature"), Symbol::intern(f)))
                .collect(),
        },
    )
    .unwrap();
    let mut dependencies = sources[indices[&cli_index]].dependencies.clone();
    dependencies.push((Symbol::intern("flux_cli"), indices[&cli_index]));
    sources.push(SourceCrate {
        root: binary,
        name: Symbol::intern("flux"),
        dependencies,
    });
    phase(root, &format!("{label}-syntax-hir"));
    let hir = flux_hir::lower_sources(&mut db, &sources);
    save(
        root,
        "loaded-files.json",
        &db.files().map(|(_, f)| &f.path).collect::<Vec<_>>(),
    );
    report(root, &format!("{label}-syntax"), &db, &db.syntax_errors());
    report(root, &format!("{label}-hir"), &db, &hir.diagnostics);
    assert!(
        db.syntax_errors().is_empty() && hir.diagnostics.is_empty(),
        "compiler syntax/HIR blockers; see {}",
        root.display()
    );
    phase(root, &format!("{label}-type"));
    let typed = flux_mir::check_file(&hir);
    report(root, &format!("{label}-type"), &db, &typed.diagnostics);
    assert!(typed.diagnostics.is_empty(), "compiler type blockers");
    phase(root, &format!("{label}-mir"));
    let mir = flux_mir::build_file(&hir, &typed);
    report(root, &format!("{label}-mir"), &db, &mir.diagnostics);
    assert!(mir.diagnostics.is_empty(), "compiler MIR blockers");
    for (label, tier) in [
        ("unoptimized", flux_codegen::OptimizationTier::None),
        ("optimized", flux_codegen::OptimizationTier::Full),
    ] {
        let files: Files =
            serde_json::from_slice(&std::fs::read(root.join("source-inventory.json")).unwrap())
                .unwrap();
        integrity(root, &files, "pre-codegen-source-integrity.json");
        phase(root, &format!("{}-codegen-{label}", "compiler"));
        let lowered = flux_codegen::lower_module(&hir, &typed, &mir);
        report(
            root,
            &format!("compiler-codegen-{label}"),
            &db,
            lowered.as_ref().err().map_or(&[][..], Vec::as_slice),
        );
        let mut module = lowered.expect("compiler codegen blockers");
        let target = flux_codegen::Target::Aarch64AppleDarwin;
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
        let executable = root.join(format!("flux-{label}"));
        flux_codegen::link_executable_with_libraries(
            &object,
            target,
            &executable,
            &hir.items.native_libraries,
        )
        .unwrap();
        cli_contract(&executable, root, label);
        for suffix in ["help", "version", "smoke"] {
            assert_eq!(
                std::fs::read(root.join(format!("{label}-{suffix}.stdout"))).unwrap(),
                std::fs::read(root.join(format!("reference-{suffix}.stdout"))).unwrap()
            );
            assert_eq!(
                std::fs::read(root.join(format!("{label}-{suffix}.stderr"))).unwrap(),
                std::fs::read(root.join(format!("reference-{suffix}.stderr"))).unwrap()
            );
        }
    }
    phase(root, &format!("{label}-native-verified"));
}

fn cli_contract(executable: &Path, root: &Path, label: &str) {
    for (suffix, arg) in [("help", "--help"), ("version", "--version")] {
        success(
            &run(
                Command::new(executable).arg0("flux").arg(arg),
                root,
                &format!("{label}-{suffix}"),
                30,
            ),
            root,
            "CLI contract",
        );
    }
    let smoke = root.join("smoke.rs");
    std::fs::write(&smoke, include_str!("fixtures/flux_compiler/smoke.rs.txt")).unwrap();
    let output = root.join(format!("{label}-smoke-executable"));
    phase(root, &format!("{label}-compile-smoke"));
    success(
        &run(
            Command::new(executable)
                .arg0("flux")
                .arg("build")
                .arg(&smoke)
                .arg("-o")
                .arg(&output),
            root,
            &format!("{label}-compile-smoke"),
            90,
        ),
        root,
        "compiler smoke build",
    );
    success(
        &run(
            &mut Command::new(output),
            root,
            &format!("{label}-smoke"),
            30,
        ),
        root,
        "compiled smoke execution",
    );
}

fn select_packages(packages: &[Value], index: usize, selected: &mut BTreeSet<usize>) {
    if !selected.insert(index) || packages[index]["proc_macro"] == true {
        return;
    }
    for edge in packages[index]["dependencies"].as_array().unwrap() {
        select_packages(packages, edge["index"].as_u64().unwrap() as usize, selected);
    }
}
