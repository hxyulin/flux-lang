//! Shared mechanics for the pinned backend source graph and its native consumers.
use crate::support;

use flux_hir::{CrateCfg, SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use support::inventory::{Files, collect, report};

const SPEC: &str = include_str!("../fixtures/keel_backend/graph.json");

/// A consumer of the same pinned, unchanged backend dependency graph.
#[derive(Clone, Copy)]
pub struct Suite {
    pub test: &'static str,
    pub output: &'static str,
    pub harness: &'static str,
    pub fixture: &'static str,
    pub prepare: Option<fn(&Path, &[Value]) -> Prepared>,
    pub default_scope: Option<&'static str>,
    pub programs: &'static [&'static str],
}

pub struct Prepared {
    pub consumer: String,
    pub inputs: Vec<PathBuf>,
}

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
fn closure(packages: &[Value], roots: &[&str]) -> BTreeSet<usize> {
    fn visit(index: usize, packages: &[Value], seen: &mut BTreeSet<usize>) {
        if !seen.insert(index) {
            return;
        }
        if packages[index]["proc_macro"] == true {
            return;
        }
        for edge in packages[index]["dependencies"].as_array().unwrap() {
            visit(edge["index"].as_u64().unwrap() as usize, packages, seen);
        }
    }
    let mut seen = BTreeSet::new();
    for name in roots {
        visit(
            packages.iter().position(|p| p["package"] == *name).unwrap(),
            packages,
            &mut seen,
        );
    }
    seen
}

pub fn run_suite(suite: Suite) {
    if let Some(root) = std::env::var_os("FLUX_BACKEND_ROOT") {
        let root = PathBuf::from(root);
        if std::env::var_os("FLUX_BACKEND_FLUX").is_some() {
            flux_core(&root, suite);
        } else {
            audit(&root, suite);
        }
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/{}/{}", suite.output, std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let output = run(
        Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", suite.test, "--nocapture"])
            .env("FLUX_BACKEND_ROOT", &root),
        &root,
        "audit",
        360,
    );
    let inventory = root.join("source-inventory.json");
    if inventory.exists() {
        let files: Files = serde_json::from_slice(&std::fs::read(inventory).unwrap()).unwrap();
        integrity(&root, &files, "parent-source-integrity.json");
    }
    success(&output, &root, "backend audit");
}

/// Run the compiled backend, then execute every program it produced. The
/// generated main functions return zero only when their semantic checks pass.
fn execute_consumer(
    root: &Path,
    executable: &Path,
    label: &str,
    sdk: &str,
    programs: &[&str],
) -> Output {
    let directory = root.join(format!("generated-{label}"));
    std::fs::create_dir_all(&directory).unwrap();
    let actual = run(
        Command::new(executable)
            .env("SDKROOT", sdk)
            .env("KEEL_SUITE_OUT", &directory),
        root,
        label,
        60,
    );
    success(&actual, root, "native backend consumer");
    for program in programs {
        let output = run(
            &mut Command::new(directory.join(program)),
            root,
            &format!("{label}-{program}"),
            20,
        );
        success(&output, root, "backend-generated program");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "unexpected generated program output: {label}/{program}"
        );
    }
    actual
}

fn audit(root: &Path, suite: Suite) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let only = std::env::var("FLUX_BACKEND_ONLY")
        .ok()
        .or_else(|| suite.default_scope.map(str::to_owned));
    assert!(
        only.as_deref()
            .is_none_or(|scope| matches!(scope, "core" | "full")),
        "expected FLUX_BACKEND_ONLY=core or full"
    );
    save(
        root,
        "selected-scope.json",
        &only.as_deref().unwrap_or("both"),
    );
    let spec: Value = serde_json::from_str(SPEC).unwrap();
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
        assert_eq!(package["source"], expected["source"]);
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
        packages.push(serde_json::json!({"package":package["name"],"version":package["version"],"manifest":manifest,"id":package["id"],"name":library["name"],"root":library["src_path"],"edition":package["edition"],"features":expected["features"],"proc_macro":expected["proc_macro"],"dependencies":expected["dependencies"]}));
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
    let prepared = match suite.prepare {
        Some(prepare) => prepare(root, &packages),
        None => Prepared {
            consumer: std::fs::read_to_string(repo.join(suite.fixture)).unwrap(),
            inputs: Vec::new(),
        },
    };
    for path in &prepared.inputs {
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }
    for path in [
        repo.join("Cargo.toml"),
        repo.join("Cargo.lock"),
        repo.join("crates/flux-hir/src/source.rs"),
        repo.join(suite.harness),
        repo.join("crates/flux-cli/tests/support/backend_graph.rs"),
        repo.join(suite.fixture),
        repo.join("crates/flux-cli/tests/fixtures/keel_backend/graph.json"),
    ] {
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }
    collect(&repo.join("library"), &mut originals);
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
        let consumer = root.join("consumer.rs");
        std::fs::write(&consumer, &prepared.consumer).unwrap();
        phase(root, "rustc-reference");
        let sdk = run(
            Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]),
            root,
            "sdk",
            10,
        );
        success(&sdk, root, "SDK discovery");
        let sdk = String::from_utf8(sdk.stdout).unwrap();
        for full in [true, false] {
            let label = if full { "full" } else { "core" };
            if only.as_deref().is_some_and(|scope| scope != label) {
                continue;
            }
            let executable = root.join(format!("reference-{label}"));
            let mut command = Command::new("rustc");
            command
                .args(["--edition=2024", "-Cpanic=abort", "-Coverflow-checks=off"])
                .arg(&consumer)
                .arg("-L")
                .arg(format!(
                    "dependency={}",
                    repo.join("target/debug/deps").display()
                ))
                .arg("-o")
                .arg(&executable);
            if full {
                command.args(["--cfg", "full_backend"]);
            }
            for name in [
                "keel-codegen",
                "keel-frontend",
                "keel-ir",
                "keel-opt",
                "keel-asm",
                "object",
                "keel-link",
            ] {
                if !full && name == "keel-link" {
                    continue;
                }
                let p = packages.iter().find(|p| p["package"] == name).unwrap();
                command.arg("--extern").arg(format!(
                    "{}={}",
                    p["name"].as_str().unwrap(),
                    libraries[p["id"].as_str().unwrap()]
                ));
            }
            success(
                &run(&mut command, root, &format!("rustc-{label}"), 60),
                root,
                "reference consumer compile",
            );
            execute_consumer(
                root,
                &executable,
                &format!("reference-{label}"),
                sdk.trim(),
                suite.programs,
            );
        }
        integrity(root, &originals, "pre-flux-source-integrity.json");
        std::fs::write(root.join("input.diagnostics"), "count=0\n").unwrap();
        let mut failed = Vec::new();
        for (label, roots) in [
            (
                "full",
                vec![
                    "keel-codegen",
                    "keel-frontend",
                    "keel-opt",
                    "keel-asm",
                    "keel-link",
                ],
            ),
            (
                "core",
                vec!["keel-codegen", "keel-frontend", "keel-opt", "keel-asm"],
            ),
        ] {
            if only.as_deref().is_some_and(|scope| scope != label) {
                continue;
            }
            let selected = closure(&packages, &roots);
            save(root, &format!("{label}-indices.json"), &selected);
            phase(root, &format!("{label}-flux"));
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--ignored", "--exact", suite.test, "--nocapture"])
                .env("FLUX_BACKEND_ROOT", root)
                .env("FLUX_BACKEND_FLUX", "1")
                .env("FLUX_BACKEND_SCOPE", label);
            // Complete frontend checks and both native optimization tiers within
            // one bounded worker; successful codegen outlasts admission-only probes.
            let output = run(&mut command, root, &format!("{label}-audit"), 240);
            if !output.status.success() {
                failed.push(label);
            }
        }
        assert!(
            failed.is_empty(),
            "backend graph blockers in {failed:?}; see {}",
            root.display()
        );
        phase(
            root,
            match only.as_deref() {
                Some("core") => "core-native-verified",
                Some("full") => "full-native-verified",
                _ => "native-verified",
            },
        );
    }));
    integrity(root, &originals, "source-integrity.json");
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

fn flux_core(root: &Path, suite: Suite) {
    let label = std::env::var("FLUX_BACKEND_SCOPE").unwrap_or_else(|_| "core".into());
    let full = label == "full";
    let packages: Vec<Value> =
        serde_json::from_slice(&std::fs::read(root.join("configuration.json")).unwrap()).unwrap();
    let selected: BTreeSet<usize> =
        serde_json::from_slice(&std::fs::read(root.join(format!("{label}-indices.json"))).unwrap())
            .unwrap();
    let scripts: BTreeMap<String, Value> =
        serde_json::from_slice(&std::fs::read(root.join("build-outputs.json")).unwrap()).unwrap();
    let indices: BTreeMap<usize, usize> = selected
        .iter()
        .enumerate()
        .map(|(i, &old)| (old, i))
        .collect();
    save(
        root,
        &format!("{label}-configuration.json"),
        &selected.iter().map(|&i| &packages[i]).collect::<Vec<_>>(),
    );
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
        let mut environment = BTreeMap::from([(
            "CARGO_MANIFEST_DIR".into(),
            Path::new(p["manifest"].as_str().unwrap())
                .parent()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        )]);
        environment.insert(
            "CARGO_PKG_NAME".into(),
            p["package"].as_str().unwrap().into(),
        );
        environment.insert(
            "CARGO_PKG_VERSION".into(),
            p["version"].as_str().unwrap().into(),
        );
        environment.insert(
            "CARGO_CRATE_NAME".into(),
            p["name"].as_str().unwrap().into(),
        );
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
    let consumer = db
        .load_with_edition(&root.join("consumer.rs"), Edition::E2024)
        .unwrap();
    if full {
        db.set_crate_cfg(
            consumer,
            CrateCfg {
                names: vec![Symbol::intern("full_backend")],
                values: Vec::new(),
            },
        )
        .unwrap();
    }
    sources.push(SourceCrate {
        root: consumer,
        name: Symbol::intern("consumer"),
        dependencies: [
            "keel-codegen",
            "keel-frontend",
            "keel-ir",
            "keel-opt",
            "keel-asm",
            "object",
            "keel-link",
        ]
        .into_iter()
        .filter(|name| full || *name != "keel-link")
        .map(|name| {
            let old = packages.iter().position(|p| p["package"] == name).unwrap();
            (
                Symbol::intern(packages[old]["name"].as_str().unwrap()),
                indices[&old],
            )
        })
        .collect(),
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
        "core syntax/HIR blockers; see {}",
        root.display()
    );
    phase(root, &format!("{label}-type"));
    let typed = flux_mir::check_file(&hir);
    report(root, &format!("{label}-type"), &db, &typed.diagnostics);
    assert!(typed.diagnostics.is_empty(), "core type blockers");
    phase(root, &format!("{label}-mir"));
    let mir = flux_mir::build_file(&hir, &typed);
    report(root, &format!("{label}-mir"), &db, &mir.diagnostics);
    assert!(mir.diagnostics.is_empty(), "core MIR blockers");
    for (label, tier) in [
        ("unoptimized", flux_codegen::OptimizationTier::None),
        ("optimized", flux_codegen::OptimizationTier::Full),
    ] {
        let files: Files =
            serde_json::from_slice(&std::fs::read(root.join("source-inventory.json")).unwrap())
                .unwrap();
        integrity(root, &files, "pre-codegen-source-integrity.json");
        phase(
            root,
            &format!("{}-codegen-{label}", if full { "full" } else { "core" }),
        );
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
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
        let actual = execute_consumer(
            root,
            &executable,
            &format!("run-flux-{label}"),
            std::fs::read_to_string(root.join("sdk.stdout"))
                .unwrap()
                .trim(),
            suite.programs,
        );
        assert_eq!(
            actual.stdout,
            std::fs::read(root.join(if full {
                "reference-full.stdout"
            } else {
                "reference-core.stdout"
            }))
            .unwrap()
        );
        assert_eq!(
            actual.stderr,
            std::fs::read(root.join(if full {
                "reference-full.stderr"
            } else {
                "reference-core.stderr"
            }))
            .unwrap()
        );
    }
    phase(root, &format!("{label}-native-verified"));
}
