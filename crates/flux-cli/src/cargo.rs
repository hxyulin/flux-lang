//! Native offline planning, Cargo-backed host preparation, and Flux source builds.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::{Command as Process, ExitCode};

use serde::{Deserialize, Serialize};

use crate::build::{self, CompileRequest, SourceInput};

pub(crate) mod native;

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
    /// Build selected workspace libraries and binaries with Flux.
    Build {
        #[command(flatten)]
        options: Options,
        #[arg(short = 'r', long = "release", short_alias = 'O')]
        optimize: bool,
        /// Use a short optimization pipeline for faster builds.
        #[arg(long, conflicts_with = "optimize")]
        fast_build: bool,
        /// Print host preparation and compilation timings.
        #[arg(long)]
        timings: bool,
    },
}

#[derive(clap::Args)]
pub(crate) struct Options {
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,
    #[arg(short = 'p', long)]
    package: Option<String>,
    #[arg(long, conflicts_with = "package")]
    workspace: bool,
    #[arg(long)]
    bin: Option<String>,
    #[arg(long)]
    lib: bool,
    #[arg(long)]
    bins: bool,
    #[arg(long, value_delimiter = ',')]
    features: Vec<String>,
    #[arg(long)]
    no_default_features: bool,
    #[arg(long)]
    offline: bool,
    /// Select the compiler used for procedural macro hosts.
    #[arg(long, value_enum, default_value_t = crate::package::MacroHostCompiler::Flux, default_value_if("cargo", "true", Some("rustc")))]
    pub(crate) macro_host_compiler: crate::package::MacroHostCompiler,
    /// Use Cargo for host preparation instead of the native package pipeline.
    #[arg(long)]
    pub(crate) cargo: bool,
    /// Maximum number of concurrent crate compilations.
    #[arg(short = 'j', long, value_parser = clap::value_parser!(u32).range(1..))]
    pub(crate) jobs: Option<u32>,
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
        let build = match command {
            Command::Build {
                optimize,
                fast_build,
                timings,
                ..
            } => Some(crate::BuildOptions {
                panic_strategy: flux_syntax::PanicStrategy::Abort,
                optimize: *optimize,
                fast_build: *fast_build,
                timings: *timings,
                debug_assertions: None,
            }),
            _ => None,
        };
        return native::prepare(options, build);
    }
    if options.workspace || options.lib || options.bins {
        return Err(
            "workspace and library target selection require native builds; remove --cargo".into(),
        );
    }
    if matches!(
        options.macro_host_compiler,
        crate::package::MacroHostCompiler::Flux
    ) {
        return Err("flux macro hosts require native preparation; remove --cargo".into());
    }
    let timed = matches!(command, Command::Build { timings: true, .. });
    let mut preparation = build::Timings::new(timed, "Cargo preparation elapsed");
    let rustc = preparation.time("rustc version", || {
        output(
            Process::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())).arg("-vV"),
            "rustc version",
        )
    })?;
    let host = rustc
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("rustc did not report its host")?
        .to_owned();
    let mut metadata = base(options, "metadata");
    metadata.args(["--filter-platform", &host, "--format-version", "1"]);
    let metadata: Metadata = preparation.time("Cargo metadata", || {
        serde_json::from_str(&output(&mut metadata, "Cargo metadata")?)
            .map_err(|e| format!("invalid Cargo metadata: {e}"))
    })?;
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
    let messages = preparation.time("Cargo host preparation", || {
        output(&mut check, "Cargo host preparation")
    })?;
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
    preparation.time("source graph planning", || {
        visit(
            &package.id,
            target,
            &metadata,
            &artifacts,
            &mut plan,
            &mut visited,
            &mut active,
        )
    })?;
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
    let Command::Build {
        optimize,
        fast_build,
        timings,
        ..
    } = command
    else {
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
    let mut macro_server = None;
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
        if macro_server.is_none() {
            macro_server = Some(preparation.time("proc-macro server discovery", || {
                crate::proc_macro::rustc_server(&plan.rustc)
            })?);
        }
        let exports = preparation.time("proc-macro exports", || {
            crate::proc_macro::exports_from_server(
                macro_server.as_deref().expect("discovered macro server"),
                &host.artifact,
                &host.source,
                host_cfg,
            )
        })?;
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
    // Cargo hands every library's link directives to the final link.
    let mut native = flux_codegen::NativeLink::default();
    for source in &plan.sources {
        let Some(script) = plan.build_scripts.get(&source.package_id) else {
            continue;
        };
        let directory = (metadata.packages.iter())
            .find(|p| p.id == source.package_id)
            .and_then(|p| p.manifest_path.parent())
            .ok_or("unknown build-script package")?;
        for library in &script.linked_libs {
            link_library(&mut native, library)?;
        }
        for path in &script.linked_paths {
            link_search(&mut native, path, directory);
        }
    }
    // Cargo's messages leave out link arguments; its saved script output has them.
    if let Some(script) = plan.build_scripts.get(&package.id) {
        let path = script.out_dir.with_file_name("output");
        let output = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let directory = package
            .manifest_path
            .parent()
            .ok_or("manifest has no parent")?;
        link_arguments(
            &mut native,
            &script_link_args(&output, &target.name)?,
            directory,
        )?;
    }
    let main = sources.last().ok_or("empty Cargo source graph")?;
    let executable = metadata
        .target_directory
        .join("flux")
        .join(if *fast_build {
            "fast"
        } else if *optimize {
            "release"
        } else {
            "debug"
        })
        .join(&target.name);
    std::fs::create_dir_all(executable.parent().expect("target output has a parent"))
        .map_err(|e| format!("cannot create executable directory: {e}"))?;
    let debug_assertions = profile_debug_assertions(
        &metadata.workspace_root.join("Cargo.toml"),
        *optimize || *fast_build,
    )?;
    let panic_strategy = profile_panic_strategy(
        &metadata.workspace_root.join("Cargo.toml"),
        *optimize || *fast_build,
    )?;
    crate::builtins::initialize_for_strategy(panic_strategy)?;
    let _strategy = crate::builtins::strategy_scope(panic_strategy)?;
    drop(preparation);
    let ok = build::compile(CompileRequest {
        file: &main.file,
        crate_name: &main.name,
        output: &executable,
        sources: &sources,
        options: crate::BuildOptions {
            panic_strategy,
            optimize: *optimize,
            fast_build: *fast_build,
            timings: *timings,
            debug_assertions: Some(debug_assertions),
        },
        native,
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
pub(crate) fn edition(value: &str) -> Result<flux_syntax::Edition, String> {
    match value {
        "2015" => Ok(flux_syntax::Edition::E2015),
        "2018" => Ok(flux_syntax::Edition::E2018),
        "2021" => Ok(flux_syntax::Edition::E2021),
        "2024" => Ok(flux_syntax::Edition::E2024),
        _ => Err(format!("unsupported edition {value}")),
    }
}
/// The `debug-assertions` setting of the Cargo profile a build uses: `release`
/// with `-O` and `dev` without, from the workspace manifest's `[profile]`
/// table, with Cargo's defaults (on for `dev`, off for `release`).
pub(crate) fn profile_debug_assertions(
    manifest: &std::path::Path,
    optimize: bool,
) -> Result<bool, String> {
    let name = if optimize { "release" } else { "dev" };
    let text = std::fs::read_to_string(manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let table: toml::Table = text
        .parse()
        .map_err(|e| format!("invalid manifest {}: {e}", manifest.display()))?;
    let setting = table
        .get("profile")
        .and_then(|profiles| profiles.get(name))
        .and_then(|profile| profile.get("debug-assertions"));
    match setting {
        None => Ok(!optimize),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("profile.{name}.debug-assertions must be a boolean")),
    }
}

pub(crate) fn parse_panic_strategy(value: &str) -> Result<flux_syntax::PanicStrategy, String> {
    match value {
        "abort" => Ok(flux_syntax::PanicStrategy::Abort),
        "unwind" => Ok(flux_syntax::PanicStrategy::Unwind),
        _ => Err(format!(
            "panic strategy must be `abort` or `unwind`, found `{value}`"
        )),
    }
}

/// Flux keeps abort as its default; an explicit Cargo profile opts into unwinding.
pub(crate) fn profile_panic_strategy(
    manifest: &std::path::Path,
    optimize: bool,
) -> Result<flux_syntax::PanicStrategy, String> {
    let name = if optimize { "release" } else { "dev" };
    let text = std::fs::read_to_string(manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let table: toml::Table = text
        .parse()
        .map_err(|e| format!("invalid TOML in {}: {e}", manifest.display()))?;
    let setting = table
        .get("profile")
        .and_then(|profiles| profiles.get(name))
        .and_then(|profile| profile.get("panic"));
    match setting {
        None => Ok(flux_syntax::PanicStrategy::Abort),
        Some(value) => {
            let value = value
                .as_str()
                .ok_or_else(|| format!("profile.{name}.panic must be `abort` or `unwind`"))?;
            parse_panic_strategy(value)
        }
    }
}

pub(crate) fn cfg(cfgs: &[String], features: &[String]) -> Result<flux_hir::CrateCfg, String> {
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

/// Adds a library request in rustc's `-l [KIND[:MODIFIERS]=]NAME` form, as
/// `cargo:rustc-link-lib` gives it.
pub(crate) fn link_library(
    native: &mut flux_codegen::NativeLink,
    value: &str,
) -> Result<(), String> {
    use flux_hir::NativeKind;
    let (kind, name) = match value.split_once('=') {
        Some((kind, name)) => (Some(kind), name),
        None => (None, value),
    };
    let (kind, modifiers) = match kind.map(|k| k.split_once(':').unwrap_or((k, ""))) {
        Some((kind, modifiers)) => (Some(kind), modifiers),
        None => (None, ""),
    };
    let kind = match kind {
        None | Some("dylib") => NativeKind::Dylib,
        Some("static") => NativeKind::Static,
        Some("framework") => NativeKind::Framework,
        Some(kind) => return Err(format!("native library kind `{kind}` is not supported yet")),
    };
    for modifier in modifiers.split(',').filter(|m| !m.is_empty()) {
        // Bundling and as-needed do not change what a whole-program link reads.
        if !matches!(
            modifier,
            "+bundle" | "-bundle" | "+as-needed" | "-as-needed" | "-whole-archive" | "-verbatim"
        ) {
            return Err(format!(
                "native library modifier `{modifier}` is not supported yet"
            ));
        }
    }
    if name.is_empty() {
        return Err(format!("empty native library name in `{value}`"));
    }
    if name.contains(':') {
        return Err(format!(
            "renamed native library `{name}` is not supported yet"
        ));
    }
    native.add_library(flux_hir::NativeLibrary {
        name: flux_intern::Symbol::intern(name),
        kind,
    });
    Ok(())
}

/// Adds a search directory in rustc's `-L [KIND=]PATH` form, as
/// `cargo:rustc-link-search` gives it. A relative path is relative to
/// `directory`, where the build script ran.
pub(crate) fn link_search(
    native: &mut flux_codegen::NativeLink,
    value: &str,
    directory: &std::path::Path,
) {
    if let Some(path) = value.strip_prefix("framework=") {
        native.add_framework_path(directory.join(path));
    } else if !value.starts_with("crate=") && !value.starts_with("dependency=") {
        // Crate and dependency directories hold Rust crates, never native libraries.
        let path = (value.strip_prefix("native="))
            .or_else(|| value.strip_prefix("all="))
            .unwrap_or(value);
        native.add_search_path(directory.join(path));
    }
}

/// The argument of a `rustc-link-arg*` build-script directive, with the
/// binary it is for (`None` for every binary). `Ok(None)` when the directive
/// is not a link argument, or names targets Flux does not build.
pub(crate) fn binary_link_arg(
    key: &str,
    value: &str,
) -> Result<Option<(Option<String>, String)>, String> {
    Ok(match key {
        "rustc-link-arg" | "rustc-link-arg-bins" => Some((None, value.into())),
        "rustc-link-arg-bin" => {
            let (binary, argument) = value
                .split_once('=')
                .ok_or("expected rustc-link-arg-bin=BIN=FLAG")?;
            Some((Some(binary.into()), argument.into()))
        }
        _ => None,
    })
}

/// The link arguments for binary `name` in a build script's standard output.
fn script_link_args(output: &str, name: &str) -> Result<Vec<String>, String> {
    let mut arguments = Vec::new();
    for line in output.lines() {
        let Some(directive) =
            (line.strip_prefix("cargo::")).or_else(|| line.strip_prefix("cargo:"))
        else {
            continue;
        };
        let Some((key, value)) = directive.split_once('=') else {
            continue;
        };
        if let Some((binary, argument)) = binary_link_arg(key, value)?
            && binary.is_none_or(|b| b == name)
        {
            arguments.push(argument);
        }
    }
    Ok(arguments)
}

/// Adds the linker arguments of `cargo:rustc-link-arg`, as far as they name
/// libraries and search directories: `-lNAME`, `-LDIR`, `-FDIR` and
/// `-framework NAME`. Flux's linker takes no other arguments.
pub(crate) fn link_arguments(
    native: &mut flux_codegen::NativeLink,
    arguments: &[String],
    directory: &std::path::Path,
) -> Result<(), String> {
    let mut arguments = arguments.iter().flat_map(|a| a.split_whitespace());
    while let Some(argument) = arguments.next() {
        if argument == "-framework" {
            let name = arguments
                .next()
                .ok_or("`-framework` needs a framework name")?;
            link_library(native, &format!("framework={name}"))?;
        } else if let Some(name) = argument.strip_prefix("-l").filter(|n| !n.is_empty()) {
            link_library(native, name)?;
        } else if let Some(path) = argument.strip_prefix("-L").filter(|p| !p.is_empty()) {
            native.add_search_path(directory.join(path));
        } else if let Some(path) = argument.strip_prefix("-F").filter(|p| !p.is_empty()) {
            native.add_framework_path(directory.join(path));
        } else {
            return Err(format!("linker argument `{argument}` is not supported yet"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn profile_panic_strategy_defaults_and_explicit_modes() {
        use flux_syntax::PanicStrategy::{Abort, Unwind};
        let directory =
            std::env::temp_dir().join(format!("flux-panic-profile-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let manifest = directory.join("Cargo.toml");
        let check = |text: &str, optimize: bool| {
            std::fs::write(&manifest, text).unwrap();
            super::profile_panic_strategy(&manifest, optimize)
        };
        assert_eq!(check("", false), Ok(Abort));
        assert_eq!(check("", true), Ok(Abort));
        let profiles = "[profile.dev]\npanic='unwind'\n[profile.release]\npanic='abort'\n";
        assert_eq!(check(profiles, false), Ok(Unwind));
        assert_eq!(check(profiles, true), Ok(Abort));
        assert!(check("[profile.dev]\npanic=true\n", false).is_err());
        assert!(check("[profile.release]\npanic='unknown'\n", true).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn profile_debug_assertions_follow_cargo_defaults_and_overrides() {
        let directory = std::env::temp_dir().join(format!("flux-profile-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let manifest = directory.join("Cargo.toml");
        let check = |text: &str, optimize: bool| {
            std::fs::write(&manifest, text).unwrap();
            super::profile_debug_assertions(&manifest, optimize)
        };
        let package = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";
        assert_eq!(check(package, false), Ok(true));
        assert_eq!(check(package, true), Ok(false));
        let set = format!(
            "{package}[profile.dev]\ndebug-assertions = false\n\
             [profile.release]\ndebug-assertions = true\n"
        );
        assert_eq!(check(&set, false), Ok(false));
        assert_eq!(check(&set, true), Ok(true));
        let invalid = format!("{package}[profile.release]\ndebug-assertions = 1\n");
        assert!(check(&invalid, true).is_err());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn link_directives_follow_rustc_syntax() {
        use flux_hir::NativeKind;
        let mut native = flux_codegen::NativeLink::default();
        let directory = std::path::Path::new("/pkg");
        super::link_library(&mut native, "static:+bundle=psm_s").unwrap();
        super::link_library(&mut native, "framework=IOKit").unwrap();
        super::link_library(&mut native, "z").unwrap();
        super::link_search(&mut native, "native=/out", directory);
        super::link_search(&mut native, "dependency=/deps", directory);
        super::link_search(&mut native, "framework=/frameworks", directory);
        super::link_search(&mut native, "relative", directory);
        super::link_arguments(
            &mut native,
            &[
                "-framework".into(),
                "Security".into(),
                "-liconv -L/lib".into(),
            ],
            directory,
        )
        .unwrap();
        let kinds: Vec<_> = (native.libraries.iter())
            .map(|l| (l.name.as_str().to_owned(), l.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("psm_s".into(), NativeKind::Static),
                ("IOKit".into(), NativeKind::Framework),
                ("z".into(), NativeKind::Dylib),
                ("Security".into(), NativeKind::Framework),
                ("iconv".into(), NativeKind::Dylib),
            ]
        );
        let paths = |p: &[std::path::PathBuf]| {
            p.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            paths(&native.search_paths),
            ["/out", "/pkg/relative", "/lib"]
        );
        assert_eq!(paths(&native.framework_paths), ["/frameworks"]);
        assert!(super::link_library(&mut native, "static:+whole-archive=a").is_err());
        assert!(super::link_library(&mut native, "raw-dylib=a").is_err());
        assert!(
            super::link_arguments(&mut native, &["-Wl,-dead_strip".into()], directory).is_err()
        );
    }
}
