//! Native offline planning, Cargo-backed host preparation, and Flux source builds.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::{Command as Process, ExitCode};

use serde::{Deserialize, Serialize};

use crate::build::{self, CompileRequest, SourceInput};

mod native;

#[derive(clap::Subcommand)]
pub(crate) enum Command {
    /// Build or run a restricted standalone package.
    Package {
        #[command(subcommand)]
        command: crate::package::Command,
    },
    /// Plan locked dependencies from local manifests and caches, without Cargo.
    Plan(Options),
    /// Fetch exact locked registry and git sources without Cargo.
    Fetch(native::FetchOptions),
    /// Resolve dependencies and generate Cargo.lock without invoking Cargo.
    GenerateLockfile(native::ResolveOptions),
    /// Prepare build scripts and host proc-macro artifacts.
    Prepare(Options),
    /// Prepare inputs, then compile a supported binary's source graph with Flux.
    Build {
        #[command(flatten)]
        options: Options,
        #[arg(short = 'O')]
        optimize: bool,
    },
}

#[derive(clap::Args)]
pub(crate) struct Options {
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,
    #[arg(short = 'p', long)]
    package: Option<String>,
    #[arg(long)]
    bin: Option<String>,
    #[arg(long, value_delimiter = ',')]
    features: Vec<String>,
    #[arg(long)]
    no_default_features: bool,
    #[arg(long)]
    offline: bool,
    /// Use Cargo for host preparation instead of the native package pipeline.
    #[arg(long)]
    pub(crate) cargo: bool,
    /// Write a versioned plan here (default: target/flux/{native,cargo}-plan.json).
    #[arg(long)]
    plan: Option<PathBuf>,
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Resolve,
    target_directory: PathBuf,
    workspace_root: PathBuf,
    workspace_default_members: Vec<String>,
}
#[derive(Deserialize)]
struct Resolve {
    root: Option<String>,
    nodes: Vec<Node>,
}
#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    manifest_path: PathBuf,
    targets: Vec<Target>,
    dependencies: Vec<Dependency>,
    features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    license_file: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
struct Target {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
    edition: String,
}
#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<Edge>,
    features: Vec<String>,
}
#[derive(Deserialize)]
struct Edge {
    name: String,
    pkg: String,
    dep_kinds: Vec<DepKind>,
}
#[derive(Deserialize)]
struct DepKind {
    kind: Option<String>,
    target: Option<String>,
}
#[derive(Deserialize)]
struct Dependency {
    name: String,
    rename: Option<String>,
    optional: bool,
    kind: Option<String>,
    target: Option<String>,
}
#[derive(Default, Deserialize, Serialize, PartialEq, Eq)]
struct Script {
    #[serde(default)]
    cfgs: Vec<String>,
    #[serde(default)]
    env: Vec<(String, String)>,
    out_dir: PathBuf,
    #[serde(default)]
    linked_libs: Vec<String>,
    #[serde(default)]
    linked_paths: Vec<String>,
}
#[derive(Deserialize)]
struct Message {
    reason: String,
    package_id: Option<String>,
    target: Option<Target>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    filenames: Vec<PathBuf>,
    #[serde(flatten)]
    fields: serde_json::Map<String, serde_json::Value>,
}
#[derive(PartialEq, Eq, Serialize)]
struct HostMacro {
    features: Vec<String>,
    name: String,
    source: PathBuf,
    edition: String,
    package_id: String,
    artifact: PathBuf,
}
#[derive(Serialize)]
struct Source {
    package_id: String,
    name: String,
    root: PathBuf,
    edition: String,
    dependencies: Vec<(String, usize)>,
    proc_macros: Vec<(String, String)>,
    features: Vec<String>,
    cfgs: Vec<String>,
    environment: BTreeMap<String, String>,
}
#[derive(Serialize)]
struct Plan {
    version: u32,
    host: String,
    rustc: String,
    sources: Vec<Source>,
    host_proc_macros: Vec<HostMacro>,
    build_scripts: BTreeMap<String, Script>,
    /// These are preparation successes, but cannot yet be compiled by Flux.
    blockers: Vec<String>,
}

pub(crate) fn execute(command: &Command) -> Result<ExitCode, String> {
    if let Command::Package { command } = command {
        return crate::package::execute(command);
    }
    if let Command::GenerateLockfile(options) = command {
        return native::generate_lockfile(options);
    }
    if let Command::Fetch(options) = command {
        return native::fetch(options);
    }
    if let Command::Plan(options) = command {
        return native::execute(options);
    }
    let (Command::Prepare(options) | Command::Build { options, .. }) = command else {
        unreachable!("native planning returned above")
    };
    if !options.cargo {
        let optimize = match command {
            Command::Build { optimize, .. } => Some(*optimize),
            _ => None,
        };
        return native::prepare(options, optimize);
    }
    let rustc = output(
        Process::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())).arg("-vV"),
        "rustc version",
    )?;
    let host = rustc
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("rustc did not report its host")?
        .to_owned();
    let mut metadata = base(options, "metadata");
    metadata.args(["--filter-platform", &host, "--format-version", "1"]);
    let metadata: Metadata = serde_json::from_str(&output(&mut metadata, "Cargo metadata")?)
        .map_err(|e| format!("invalid Cargo metadata: {e}"))?;
    let package = select_package(options, &metadata)?;
    let target = select_binary(options, package)?;
    let mut check = base(options, "check");
    check.args([
        "-p",
        &package.id,
        "--bin",
        &target.name,
        "--target",
        &host,
        "--message-format=json",
    ]);
    let messages = output(&mut check, "Cargo host preparation")?;
    let HostInputs {
        scripts,
        artifacts,
        macros: host_proc_macros,
    } = inputs(&messages, &metadata, &host)?;
    let mut plan = Plan {
        version: 2,
        host,
        rustc,
        sources: Vec::new(),
        host_proc_macros,
        build_scripts: scripts,
        blockers: Vec::new(),
    };
    let mut visited = BTreeMap::new();
    let mut active = BTreeSet::new();
    visit(
        &package.id,
        target,
        &metadata,
        &artifacts,
        &mut plan,
        &mut visited,
        &mut active,
    )?;
    for (id, script) in &plan.build_scripts {
        if !script.linked_libs.is_empty() || !script.linked_paths.is_empty() {
            plan.blockers.push(format!(
                "native linker directives for {id} require Flux linker integration"
            ));
        }
    }
    let path = options
        .plan
        .clone()
        .unwrap_or_else(|| metadata.target_directory.join("flux/cargo-plan.json"));
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create plan directory: {e}"))?;
    }
    let json = serde_json::to_vec_pretty(&plan).map_err(|e| format!("cannot encode plan: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    eprintln!(
        "prepared {} source crates and {} host proc macros: {}",
        plan.sources.len(),
        plan.host_proc_macros.len(),
        path.display()
    );
    let Command::Build { optimize, .. } = command else {
        return Ok(ExitCode::SUCCESS);
    };
    if !plan.blockers.is_empty() {
        return Err(plan.blockers.join("; "));
    }
    if !cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )) {
        return Err("Cargo source builds support only arm64 macOS and x86-64 Linux hosts".into());
    }
    let mut sources = Vec::new();
    let mut hosts = BTreeMap::new();
    for host in &plan.host_proc_macros {
        if !plan.sources.iter().any(|source| {
            source
                .proc_macros
                .iter()
                .any(|(_, id)| id == &host.package_id)
        }) {
            continue;
        }
        let cfgs = plan
            .build_scripts
            .get(&host.package_id)
            .map_or(&[][..], |s| s.cfgs.as_slice());
        let mut host_cfg = cfg(cfgs, &host.features)?;
        host_cfg
            .names
            .push(flux_intern::Symbol::intern("proc_macro"));
        let exports =
            crate::proc_macro::exports(&plan.rustc, &host.artifact, &host.source, host_cfg)?;
        hosts.insert(host.package_id.as_str(), sources.len());
        sources.push(SourceInput {
            compilation_dir: None,
            package_identity: None,
            host_macros: Some(exports),
            file: host.artifact.clone(),
            name: host.name.clone(),
            edition: edition(&host.edition)?,
            dependencies: Vec::new(),
            cfg: Default::default(),
            environment: Some(BTreeMap::new()),
        });
    }
    let offset = sources.len();
    for source in &plan.sources {
        let mut dependencies = source
            .dependencies
            .iter()
            .map(|(name, index)| (name.clone(), index + offset))
            .collect::<Vec<_>>();
        for (alias, id) in &source.proc_macros {
            dependencies.push((
                alias.clone(),
                *hosts
                    .get(id.as_str())
                    .ok_or("missing host proc-macro artifact")?,
            ));
        }
        sources.push(SourceInput {
            compilation_dir: Some(metadata.workspace_root.clone()),
            package_identity: None,
            host_macros: None,
            edition: edition(&source.edition)?,
            file: source.root.clone(),
            name: source.name.clone(),
            dependencies,
            cfg: cfg(&source.cfgs, &source.features)?,
            environment: Some(source.environment.clone()),
        });
    }
    let main = sources.last().ok_or("empty Cargo source graph")?;
    let executable = metadata.target_directory.join("flux").join(&target.name);
    std::fs::create_dir_all(executable.parent().expect("target output has a parent"))
        .map_err(|e| format!("cannot create executable directory: {e}"))?;
    let ok = build::compile(CompileRequest {
        file: &main.file,
        crate_name: &main.name,
        output: &executable,
        sources: &sources,
        options: crate::BuildOptions {
            optimize: *optimize,
            timings: false,
        },
    })?;
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn base(options: &Options, action: &str) -> Process {
    let mut c = Process::new("cargo");
    c.arg(action)
        .arg("--manifest-path")
        .arg(&options.manifest_path)
        .arg("--locked");
    if options.offline {
        c.arg("--offline");
    }
    if options.no_default_features {
        c.arg("--no-default-features");
    }
    if !options.features.is_empty() {
        c.arg("--features").arg(options.features.join(","));
    }
    c
}
fn output(command: &mut Process, label: &str) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|e| format!("cannot run {label}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{label} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("invalid UTF-8 from {label}: {e}"))
}
fn select_package<'a>(options: &Options, metadata: &'a Metadata) -> Result<&'a Package, String> {
    let id = metadata.resolve.root.as_ref().or_else(|| {
        (metadata.workspace_default_members.len() == 1)
            .then(|| &metadata.workspace_default_members[0])
    });
    let matches = metadata
        .packages
        .iter()
        .filter(|p| {
            options.package.as_ref().map_or_else(
                || Some(&p.id) == id,
                |name| name == &p.name || name == &p.id,
            )
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [p] => Ok(p),
        _ => Err("select one package with --package".into()),
    }
}
fn select_binary<'a>(options: &Options, package: &'a Package) -> Result<&'a Target, String> {
    let matches = package
        .targets
        .iter()
        .filter(|t| {
            t.kind.iter().any(|k| k == "bin")
                && options.bin.as_ref().is_none_or(|name| name == &t.name)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [t] => Ok(t),
        _ => Err("select one binary with --bin".into()),
    }
}

type Artifacts = BTreeMap<(String, PathBuf), Vec<String>>;
struct HostInputs {
    scripts: BTreeMap<String, Script>,
    artifacts: Artifacts,
    macros: Vec<HostMacro>,
}
fn inputs(messages: &str, metadata: &Metadata, host: &str) -> Result<HostInputs, String> {
    let mut scripts = BTreeMap::new();
    let mut artifacts = BTreeMap::new();
    let mut macros = Vec::new();
    for line in messages.lines().filter(|line| line.starts_with('{')) {
        let message: Message =
            serde_json::from_str(line).map_err(|e| format!("invalid Cargo message: {e}"))?;
        let Some(id) = message.package_id else {
            continue;
        };
        if message.reason == "build-script-executed" {
            let script: Script = serde_json::from_value(serde_json::Value::Object(message.fields))
                .map_err(|e| format!("invalid build-script result: {e}"))?;
            if scripts.get(&id).is_some_and(|previous| previous != &script) {
                return Err(format!(
                    "distinct host/target build-script units for {id} require unit-level graph support"
                ));
            }
            scripts.insert(id, script);
        } else if message.reason == "compiler-artifact" {
            let Some(target) = message.target else {
                continue;
            };
            if target.kind.iter().any(|k| k == "proc-macro") {
                let artifact = message
                    .filenames
                    .iter()
                    .find(|p| {
                        matches!(
                            p.extension().and_then(|s| s.to_str()),
                            Some("dylib" | "so" | "dll")
                        )
                    })
                    .ok_or_else(|| format!("missing host proc-macro artifact for {id}"))?;
                let host_macro = HostMacro {
                    features: message.features,
                    name: target.name,
                    source: target.src_path,
                    edition: target.edition,
                    package_id: id,
                    artifact: artifact.clone(),
                };
                if let Some(previous) = macros
                    .iter()
                    .find(|previous: &&HostMacro| previous.package_id == host_macro.package_id)
                {
                    if previous != &host_macro {
                        return Err(format!(
                            "distinct host proc-macro units for {} require unit-level graph support",
                            host_macro.package_id
                        ));
                    }
                } else {
                    macros.push(host_macro);
                }
            } else if message
                .filenames
                .iter()
                .any(|p| p.starts_with(metadata.target_directory.join(host)))
            {
                artifacts.insert((id, target.src_path), message.features);
            }
        }
    }
    Ok(HostInputs {
        scripts,
        artifacts,
        macros,
    })
}

fn visit(
    id: &str,
    target: &Target,
    metadata: &Metadata,
    artifacts: &Artifacts,
    plan: &mut Plan,
    visited: &mut BTreeMap<String, usize>,
    active: &mut BTreeSet<String>,
) -> Result<usize, String> {
    if let Some(&index) = visited.get(id) {
        return Ok(index);
    }
    if !active.insert(id.to_owned()) {
        return Err(format!("cyclic target source dependencies at {id}"));
    }
    let package = metadata
        .packages
        .iter()
        .find(|p| p.id == id)
        .ok_or("unknown package ID")?;
    let node = metadata
        .resolve
        .nodes
        .iter()
        .find(|n| n.id == id)
        .ok_or("missing resolved package")?;
    let features = artifacts
        .get(&(id.to_owned(), target.src_path.clone()))
        .ok_or_else(|| format!("Cargo did not check target {} for {id}", target.name))?
        .clone();
    if features.iter().collect::<BTreeSet<_>>() != node.features.iter().collect::<BTreeSet<_>>() {
        return Err(format!(
            "resolved metadata and target features differ for {id}; unit-level graph support is required"
        ));
    }
    let mut dependencies = Vec::new();
    let mut proc_macros = Vec::new();
    for edge in &node.deps {
        if !edge.dep_kinds.iter().any(|k| k.kind.is_none()) {
            continue;
        }
        let dependency = metadata
            .packages
            .iter()
            .find(|p| p.id == edge.pkg)
            .ok_or("unknown dependency ID")?;
        let library = dependency
            .targets
            .iter()
            .find(|t| {
                t.kind
                    .iter()
                    .any(|k| matches!(k.as_str(), "lib" | "rlib" | "proc-macro"))
            })
            .ok_or_else(|| {
                format!(
                    "dependency {} has no supported library target",
                    dependency.name
                )
            })?;
        let declarations = package
            .dependencies
            .iter()
            .filter(|decl| {
                decl.name == dependency.name
                    && decl
                        .rename
                        .as_deref()
                        .unwrap_or(&library.name)
                        .replace('-', "_")
                        == edge.name
                    && edge.dep_kinds.iter().any(|kind| {
                        kind.kind.is_none() && decl.kind.is_none() && kind.target == decl.target
                    })
            })
            .collect::<Vec<_>>();
        if declarations.is_empty() {
            return Err(format!(
                "missing dependency declaration for {} in {id}",
                edge.name
            ));
        }
        if !declarations.iter().any(|decl| {
            !decl.optional
                || optional_dependency_enabled(
                    package,
                    &features,
                    decl.rename.as_deref().unwrap_or(&decl.name),
                )
        }) {
            continue;
        }
        if library.kind.iter().any(|k| k == "proc-macro") {
            proc_macros.push((edge.name.clone(), edge.pkg.clone()));
            continue;
        }
        let index = visit(
            &edge.pkg, library, metadata, artifacts, plan, visited, active,
        )?;
        dependencies.push((edge.name.clone(), index));
    }
    if target.kind.iter().any(|k| k == "bin")
        && let Some(library) = package
            .targets
            .iter()
            .find(|t| t.kind.iter().any(|k| k == "lib" || k == "rlib"))
    {
        // A binary also sees its own package library under the library name.
        active.remove(id);
        let index = visit(id, library, metadata, artifacts, plan, visited, active)?;
        dependencies.push((library.name.clone(), index));
    }
    let mut environment = BTreeMap::from([
        (
            "CARGO_MANIFEST_DIR".into(),
            package
                .manifest_path
                .parent()
                .ok_or("manifest has no parent")?
                .to_string_lossy()
                .into_owned(),
        ),
        ("CARGO_PKG_NAME".into(), package.name.clone()),
        ("CARGO_PKG_VERSION".into(), package.version.clone()),
        ("CARGO_CRATE_NAME".into(), target.name.replace('-', "_")),
    ]);
    let without_build = package
        .version
        .split('+')
        .next()
        .expect("Cargo package version is nonempty");
    let (version, pre) = without_build.split_once('-').unwrap_or((without_build, ""));
    let mut numbers = version.split('.');
    for (key, value) in [
        ("CARGO_PKG_VERSION_MAJOR", numbers.next().unwrap_or("")),
        ("CARGO_PKG_VERSION_MINOR", numbers.next().unwrap_or("")),
        ("CARGO_PKG_VERSION_PATCH", numbers.next().unwrap_or("")),
        ("CARGO_PKG_VERSION_PRE", pre),
        (
            "CARGO_PKG_DESCRIPTION",
            package.description.as_deref().unwrap_or(""),
        ),
        (
            "CARGO_PKG_HOMEPAGE",
            package.homepage.as_deref().unwrap_or(""),
        ),
        (
            "CARGO_PKG_REPOSITORY",
            package.repository.as_deref().unwrap_or(""),
        ),
        (
            "CARGO_PKG_LICENSE",
            package.license.as_deref().unwrap_or(""),
        ),
        (
            "CARGO_PKG_LICENSE_FILE",
            package.license_file.as_deref().unwrap_or(""),
        ),
    ] {
        environment.insert(key.into(), value.into());
    }
    environment.insert("CARGO_PKG_AUTHORS".into(), package.authors.join(":"));
    if target.kind.iter().any(|k| k == "bin") {
        environment.insert("CARGO_BIN_NAME".into(), target.name.clone());
    }
    let mut cfgs = Vec::new();
    if let Some(script) = plan.build_scripts.get(id) {
        environment.extend(script.env.iter().cloned());
        environment.insert(
            "OUT_DIR".into(),
            script.out_dir.to_string_lossy().into_owned(),
        );
        cfgs.clone_from(&script.cfgs);
    }
    let index = plan.sources.len();
    plan.sources.push(Source {
        package_id: id.into(),
        name: target.name.replace('-', "_"),
        root: target.src_path.clone(),
        edition: target.edition.clone(),
        dependencies,
        proc_macros,
        features,
        cfgs,
        environment,
    });
    visited.insert(id.into(), index);
    active.remove(id);
    Ok(index)
}
fn optional_dependency_enabled(package: &Package, features: &[String], name: &str) -> bool {
    // Metadata can retain weak `dep?/feature` edges that Cargo did not build.
    // Cargo expands implicit optional features to `dep:name` in this table.
    features.iter().any(|feature| {
        package.features.get(feature).is_some_and(|values| {
            values.iter().any(|value| {
                value.strip_prefix("dep:") == Some(name)
                    || value.split_once('/').is_some_and(|(dep, _)| dep == name)
            })
        })
    })
}
fn edition(value: &str) -> Result<flux_syntax::Edition, String> {
    match value {
        "2015" => Ok(flux_syntax::Edition::E2015),
        "2018" => Ok(flux_syntax::Edition::E2018),
        "2021" => Ok(flux_syntax::Edition::E2021),
        "2024" => Ok(flux_syntax::Edition::E2024),
        _ => Err(format!("unsupported edition {value}")),
    }
}
fn cfg(cfgs: &[String], features: &[String]) -> Result<flux_hir::CrateCfg, String> {
    use flux_intern::Symbol;
    let mut cfg = flux_hir::CrateCfg::default();
    cfg.values.extend(
        features
            .iter()
            .map(|f| (Symbol::intern("feature"), Symbol::intern(f))),
    );
    for text in cfgs {
        if let Some((key, value)) = text.split_once('=') {
            let value: String = serde_json::from_str(value)
                .map_err(|e| format!("invalid build cfg {text}: {e}"))?;
            cfg.values
                .push((Symbol::intern(key), Symbol::intern(&value)));
        } else {
            cfg.names.push(Symbol::intern(text));
        }
    }
    Ok(cfg)
}
