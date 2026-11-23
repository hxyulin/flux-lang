//! Flux build-script compilation and native host preparation.
mod script_cache;
mod toolchain_cache;

use super::{Domain, Options, Plan, PlannedPackage, Result, Target};
use crate::build::{CompileRequest, SourceInput};
use crate::flib::semantic::FreshArtifacts;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::OnceLock;

#[derive(Clone, Default, Serialize)]
struct ScriptOutput {
    cfgs: Vec<String>,
    check_cfgs: Vec<String>,
    environment: BTreeMap<String, String>,
    metadata: BTreeMap<String, String>,
    /// `-l` and `-L` requests, as rustc reads them.
    linker: Vec<(String, String)>,
    /// `rustc-link-arg` arguments for the package's binaries: for every
    /// binary (`None`), or for the one named.
    link_args: Vec<(Option<String>, String)>,
    out_dir: PathBuf,
}
#[derive(Clone, Default, Serialize)]
struct Output {
    artifact: Option<PathBuf>,
    script: Option<ScriptOutput>,
    #[serde(skip)]
    macros: Option<Vec<flux_hir::ProcMacro>>,
}
#[derive(Serialize)]
struct Prepared {
    version: u32,
    kind: &'static str,
    rustc: String,
    macro_host_compiler: &'static str,
    plan: Plan,
    outputs: Vec<Output>,
}

/// Prepares host tools, then builds the plan's binary when `build` is given.
/// `start`, set when timings are requested, is when planning began.
pub(super) fn run(
    plan: Plan,
    options: &Options,
    build: Option<crate::BuildOptions>,
    start: Option<std::time::Instant>,
) -> Result<ExitCode> {
    std::thread::scope(|scope| run_scoped(scope, plan, options, build, start))
}

fn run_scoped<'scope>(
    scope: &'scope std::thread::Scope<'scope, '_>,
    plan: Plan,
    options: &Options,
    build: Option<crate::BuildOptions>,
    start: Option<std::time::Instant>,
) -> Result<ExitCode> {
    let native_macros = matches!(
        options.macro_host_compiler,
        crate::package::MacroHostCompiler::Flux
    );
    let mut rustc_units = BTreeSet::new();
    for (id, unit) in plan.units.iter().enumerate() {
        if !native_macros && unit.domain == Domain::Host && unit.target.kind == "proc-macro" {
            rustc_dependencies(&plan, id, &mut rustc_units);
        }
    }
    let jobs = options.jobs.map_or_else(
        || std::thread::available_parallelism().map_or(1, |n| n.get()),
        |n| n as usize,
    );
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let prefetch_server = jobs > 1
        && build.is_some()
        && !native_macros
        && plan.units.iter().any(|unit| {
            unit.domain == Domain::Target
                && unit
                    .dependencies
                    .iter()
                    .any(|(_, id)| plan.units[*id].target.kind == "proc-macro")
        });
    // The profile, output directory and dependency order do not need rustc's
    // identity. Resolve it while preparing them, then check it before any work.
    enum Identity<'scope> {
        Ready(HostToolchain<'scope>),
        Pending(std::thread::ScopedJoinHandle<'scope, Result<HostToolchain<'scope>>>),
    }
    let identity = match (rustc_units.is_empty(), jobs > 1) {
        (true, _) => Identity::Ready(HostToolchain {
            rustc,
            version: String::new(),
            server: None,
        }),
        (false, false) => Identity::Ready(host_rustc_identity(scope, &rustc, None)?),
        (false, true) => Identity::Pending({
            let cache = prefetch_server.then(|| {
                plan.workspace_root
                    .join("target/flux/native-host/rustc-queries-v1")
            });
            scope.spawn(move || host_rustc_identity(scope, &rustc, cache.as_deref()))
        }),
    };
    // Build scripts and the target graph follow the Cargo profile: `release`
    // with `--release`, `dev` otherwise. An explicit setting overrides it.
    let command_options = options;
    let optimize = build.is_some_and(|b| b.optimized());
    let manifest = plan.workspace_root.join("Cargo.toml");
    let host_profile = (!rustc_units.is_empty())
        .then(|| RustcHostProfile::read(&manifest, optimize))
        .transpose()?;
    let debug_assertions = match build.and_then(|b| b.debug_assertions) {
        Some(enabled) => enabled,
        None => crate::cargo::profile_debug_assertions(&manifest, optimize)?,
    };
    let panic_strategy = if options.test_mode {
        flux_syntax::PanicStrategy::Unwind
    } else {
        crate::cargo::profile_panic_strategy(&manifest, optimize)?
    };
    let shared_build_scripts = shared_build_scripts(&plan);
    if build.is_some() {
        crate::builtins::select_for_strategy(panic_strategy)?;
    } else if !shared_build_scripts.is_empty() {
        // Direct-source and shared-library workers must agree on the
        // builtins before either can enter the compiler.
        crate::builtins::select_for_strategy(flux_syntax::PanicStrategy::Abort)?;
    }
    let _strategy = crate::builtins::strategy_scope(panic_strategy)?;
    let build = build.map(|b| crate::BuildOptions {
        panic_strategy,
        debug_assertions: Some(debug_assertions),
        ..b
    });
    let cfg = crate::target::cfg_with_panic(debug_assertions, panic_strategy)?
        .into_iter()
        .collect::<Vec<_>>()
        .join("\n");
    // Stable paths: outputs depend only on their inputs, and fresh rustc
    // artifacts from an earlier preparation are reused.
    let root = plan.workspace_root.join("target/flux/native-host");
    std::fs::create_dir_all(root.join("deps")).map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let _lock = lock_directory(&root)?;
    let selected: Vec<bool> = (plan.units.iter().enumerate())
        .map(|(index, unit)| {
            unit.domain == Domain::Host
                && (unit.target.kind == "build-script"
                    || unit.target.kind == "proc-macro"
                    || rustc_units.contains(&index))
        })
        .collect();
    let serial_macro_hosts = native_macros
        && plan
            .units
            .iter()
            .any(|unit| unit.target.kind == "proc-macro");
    let waits: Vec<Vec<usize>> = (0..plan.units.len())
        .map(|index| {
            let mut waits = prerequisites(&plan, index);
            let kind = plan.units[index].target.kind.as_str();
            if kind == "build-script" || (native_macros && kind == "proc-macro") {
                flux_host_prerequisites(&plan, index, &mut waits);
            }
            waits
        })
        .collect();
    let mut outputs: Vec<Output> = plan.units.iter().map(|_| Output::default()).collect();
    let mut done: Vec<bool> = selected.iter().map(|s| !s).collect();
    let mut started = done.clone();
    let mut reservations = vec![0; plan.units.len()];
    let HostToolchain {
        rustc,
        version,
        server,
    } = match identity {
        Identity::Ready(toolchain) => toolchain,
        Identity::Pending(pending) => pending.join().expect("rustc identity worker panicked")?,
    };
    let early_root = build.filter(|_| {
        jobs > 1
            && !native_macros
            && selected.iter().any(|&host| host)
            && plan
                .root_units
                .iter()
                .all(|&root| plan.units[root].target.kind == "lib")
    });
    let early_parent = early_root.map(|options| {
        plan.workspace_root
            .join("target/flux")
            .join(options.profile_name())
    });
    let mut early_stage = None;
    let mut early_libraries = BTreeMap::new();
    let mut early_considered = BTreeSet::new();
    let early_artifacts = early_parent.as_deref().map(FreshArtifacts::for_outputs);
    let host_artifacts = FreshArtifacts::for_outputs(&root);
    let host_sdk = OnceLock::new();
    let (finished, completions) = std::sync::mpsc::channel::<(usize, Result<Option<Output>>)>();
    let host = HostSettings {
        version: &version,
        cfg: &cfg,
        rustc: &rustc,
        native_macros,
        debug_assertions,
        release: optimize,
        jobs,
        compile_jobs: 1,
        optimize_scripts: options.optimize_build_scripts,
        timings: start.is_some(),
        shared_dependencies: false,
        artifacts: &host_artifacts,
        sdk: &host_sdk,
    };
    let plan_ref = &plan;
    let output_root = root.as_path();
    let macro_server = OnceLock::new();
    std::thread::scope(|scope| -> Result<()> {
        let mut running = 0;
        let mut failure = None;
        let mut early_failure = None;
        let mut received = None;
        loop {
            for (index, result) in received.take().into_iter().chain(completions.try_iter()) {
                running -= std::mem::take(&mut reservations[index]);
                match result {
                    Ok(output) => {
                        if let Some(output) = output {
                            outputs[index] = output;
                        }
                        done[index] = true;
                    }
                    Err(error) => {
                        if early_considered.contains(&index) {
                            early_failure.get_or_insert(error);
                        } else {
                            failure.get_or_insert(error);
                        }
                    }
                }
            }
            if failure.is_none() {
                let ready = |started: &[bool], index: usize| {
                    !started[index] && waits[index].iter().all(|d| done[*d])
                };
                let script = |index: usize| plan.units[index].target.kind == "build-script";
                for index in 0..plan.units.len() {
                    if running == jobs {
                        break;
                    }
                    if script(index) || native_macros || !ready(&started, index) {
                        continue;
                    }
                    let work = match rustc_unit(
                        &plan,
                        &outputs,
                        index,
                        &root,
                        &rustc,
                        &version,
                        host_profile.as_ref().expect("rustc host unit selected"),
                    ) {
                        Ok(work) => work,
                        Err(error) => {
                            failure = Some(error);
                            break;
                        }
                    };
                    outputs[index].artifact = Some(work.artifact.clone());
                    started[index] = true;
                    running += 1;
                    reservations[index] = 1;
                    let sender = finished.clone();
                    scope.spawn(move || {
                        let mut completion = super::build::JobCompletion {
                            sender,
                            index,
                            result: None,
                        };
                        completion.result = Some(work.run().map(|()| None));
                    });
                }
                if failure.is_none() {
                    if serial_macro_hosts {
                        // Keep the Flux macro-host path in its original order.
                        if let Some(index) = (0..plan.units.len()).find(|&i| ready(&started, i)) {
                            started[index] = true;
                            let host = HostSettings {
                                shared_dependencies: shared_build_scripts.contains(&index),
                                compile_jobs: jobs.min(4),
                                ..host
                            };
                            match flux_unit(&plan, &outputs, index, &root, &host) {
                                Ok(output) => {
                                    outputs[index] = output;
                                    done[index] = true;
                                }
                                Err(error) => failure = Some(error),
                            }
                            continue;
                        }
                    } else {
                        let ready_scripts: Vec<_> = (0..plan.units.len())
                            .filter(|&index| script(index) && ready(&started, index))
                            .collect();
                        let ready_count = ready_scripts.len();
                        for (position, index) in ready_scripts.into_iter().enumerate() {
                            if running == jobs {
                                break;
                            }
                            let shared_dependencies = shared_build_scripts.contains(&index);
                            // Leave a slot for each other ready script, then lend
                            // spare slots to this script's dependency scheduler.
                            let other_scripts = ready_count - position - 1;
                            let compile_jobs = if shared_dependencies {
                                (jobs - running).saturating_sub(other_scripts).clamp(1, 4)
                            } else {
                                1
                            };
                            // The worker reads completed outputs. The coordinator
                            // publishes this script's output only after it succeeds.
                            let inputs = outputs.clone();
                            let host = HostSettings {
                                shared_dependencies,
                                compile_jobs,
                                ..host
                            };
                            let sender = finished.clone();
                            std::thread::Builder::new()
                                .stack_size(8 * 1024 * 1024)
                                .spawn_scoped(scope, move || {
                                    let mut completion = super::build::JobCompletion {
                                        sender,
                                        index,
                                        result: None,
                                    };
                                    completion.result = Some(
                                        flux_unit(plan_ref, &inputs, index, output_root, &host)
                                            .map(Some),
                                    );
                                })
                                .map_err(|error| format!("cannot start build script: {error}"))?;
                            started[index] = true;
                            running += compile_jobs;
                            reservations[index] = compile_jobs;
                        }
                    }
                }
            }
            if failure.is_none()
                && early_failure.is_none()
                && running != 0
                && let Some(options) = early_root
            {
                let parent = early_parent.as_deref().expect("early target directory");
                let artifacts = early_artifacts.as_ref().expect("early target cache");
                for (index, unit) in plan.units.iter().enumerate() {
                    if running == jobs {
                        break;
                    }
                    if unit.domain != Domain::Target
                        || unit.target.kind != "lib"
                        || !unit.dependencies.is_empty()
                        || plan.root_units.contains(&index)
                        || !waits[index].iter().all(|&dependency| done[dependency])
                        || !early_considered.insert(index)
                    {
                        continue;
                    }
                    let script = unit.build_script.and_then(|id| outputs[id].script.as_ref());
                    let cfg = crate::cargo::cfg(
                        script.map_or(&[], |script| script.cfgs.as_slice()),
                        &unit.features,
                    )?;
                    let source = target_source(
                        &plan,
                        index,
                        script,
                        cfg,
                        None,
                        unit.target.root.clone(),
                        Vec::new(),
                    )?;
                    let work =
                        super::build::EarlyLibrary::new(&source, options, &parent.join("deps"))?;
                    if work.destination().exists()
                        || early_libraries.contains_key(work.destination())
                    {
                        continue;
                    }
                    if early_stage.is_none() {
                        early_stage = Some(super::fetch::Temporary::with_prefix(
                            &parent.join("deps"),
                            "early-target",
                        )?);
                    }
                    let stage = early_stage
                        .as_ref()
                        .expect("early staging directory")
                        .0
                        .join(format!("target-{index}.flib"));
                    early_libraries.insert(work.destination().to_owned(), stage.clone());
                    let sender = finished.clone();
                    std::thread::Builder::new()
                        .stack_size(8 * 1024 * 1024)
                        .spawn_scoped(scope, move || {
                            let mut completion = super::build::JobCompletion {
                                sender,
                                index,
                                result: None,
                            };
                            completion.result =
                                Some(work.compile(&stage, artifacts).and_then(|ok| {
                                    if ok {
                                        Ok(None)
                                    } else {
                                        Err(format!("compilation of {} failed", source.name))
                                    }
                                }));
                        })
                        .map_err(|error| format!("cannot start target library: {error}"))?;
                    running += 1;
                    reservations[index] = 1;
                }
            }
            if running == 0 {
                break;
            }
            received = Some(completions.recv().map_err(|e| e.to_string())?);
        }
        failure.or(early_failure).map_or(Ok(()), Err)
    })?;
    if done.iter().any(|d| !d) {
        return Err("native host units depend on each other in a cycle".into());
    }
    let prepared = Prepared {
        version: 1,
        kind: "flux-native-prepared",
        rustc: version,
        macro_host_compiler: if native_macros { "flux" } else { "rustc" },
        plan,
        outputs,
    };
    let path = options.plan.clone().unwrap_or_else(|| {
        prepared
            .plan
            .workspace_root
            .join("target/flux/native-prepared-plan.json")
    });
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&prepared).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    eprintln!(
        "prepared native host tools without Cargo: {}",
        path.display()
    );
    match build {
        Some(options) => {
            if let Some(server) = server {
                // The sysroot query can outlast -vV and run alongside host
                // jobs. A failed query is reported only if a macro needs it.
                let result =
                    crate::proc_macro::rustc_server_with(&prepared.rustc, || match server {
                        ServerQuery::Cached(path) => Ok(path),
                        ServerQuery::Pending(query, cache) => {
                            let path = query.join().expect("rustc sysroot worker panicked")?;
                            if let Some(cache) = cache {
                                cache.write(&prepared.rustc, &path);
                            }
                            Ok(path)
                        }
                    });
                let _ = macro_server.set(result);
            }
            if let Some(start) = start {
                eprintln!(
                    "{:<38}  {:>10.2?}",
                    "planning + native host preparation",
                    start.elapsed()
                );
            }
            compile_plan(
                &prepared,
                options,
                jobs,
                command_options,
                &macro_server,
                &early_libraries,
            )
        }
        None => Ok(ExitCode::SUCCESS),
    }
}

struct HostToolchain<'scope> {
    rustc: OsString,
    version: String,
    server: Option<ServerQuery<'scope>>,
}

enum ServerQuery<'scope> {
    Cached(PathBuf),
    Pending(
        std::thread::ScopedJoinHandle<'scope, Result<PathBuf>>,
        Option<toolchain_cache::QueryCache>,
    ),
}

fn host_rustc_identity<'scope>(
    scope: &'scope std::thread::Scope<'scope, '_>,
    requested: &OsStr,
    cache_path: Option<&Path>,
) -> Result<HostToolchain<'scope>> {
    let rustc = resolve_host_rustc(requested)?;
    let cache = cache_path.and_then(|path| toolchain_cache::QueryCache::capture(path, &rustc));
    if let Some((version, server)) = cache.as_ref().and_then(toolchain_cache::QueryCache::read) {
        return Ok(HostToolchain {
            rustc,
            version,
            server: Some(ServerQuery::Cached(server)),
        });
    }
    let server = cache_path.map(|_| {
        let rustc = rustc.clone();
        let pending = scope.spawn(move || {
            crate::flib::time("rustc sysroot query", || {
                crate::proc_macro::query_rustc_server(&rustc)
            })
        });
        ServerQuery::Pending(pending, cache)
    });
    let version = crate::flib::time("rustc version query", || {
        crate::cargo::output(Command::new(&rustc).arg("-vV"), "rustc version")
    })?;
    Ok(HostToolchain {
        rustc,
        version,
        server,
    })
}

#[cfg(unix)]
fn resolve_host_rustc(rustc: &OsStr) -> Result<OsString> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let path = Path::new(rustc);
    let program = if path.file_name() == Some(rustc) {
        let Some(program) = std::env::var_os("PATH").and_then(|dirs| {
            std::env::split_paths(&dirs)
                .map(|dir| dir.join(path))
                .find(|path| {
                    path.metadata()
                        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                })
        }) else {
            return Ok(rustc.to_owned());
        };
        program
    } else {
        path.to_owned()
    };
    let program = std::path::absolute(program).map_err(|e| e.to_string())?;
    let sibling = program.with_file_name("rustup");
    let rustup = if program
        .metadata()
        .ok()
        .zip(sibling.metadata().ok())
        .is_some_and(|(a, b)| a.dev() == b.dev() && a.ino() == b.ino())
    {
        Some(sibling)
    } else {
        program
            .canonicalize()
            .ok()
            .filter(|path| path.file_name() == Some(OsStr::new("rustup")))
    };
    // Resolve Rustup's proxy before each worker changes to its package directory.
    // Explicit compiler binaries and wrappers keep their original entry point.
    if let Some(rustup) = rustup {
        let path = crate::cargo::output(
            Command::new(rustup).args(["which", "rustc"]),
            "rustup rustc selection",
        )?;
        return Ok(path.trim_end().into());
    }
    Ok(program.into_os_string())
}

#[cfg(not(unix))]
fn resolve_host_rustc(rustc: &OsStr) -> Result<OsString> {
    Ok(rustc.to_owned())
}

/// Serializes preparations of one workspace, which share its host directory.
fn lock_directory(root: &Path) -> Result<std::fs::File> {
    let file = std::fs::File::create(root.join(".lock")).map_err(|e| e.to_string())?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            eprintln!(
                "waiting for another native host preparation in {}",
                root.display()
            );
            file.lock().map_err(|e| e.to_string())?;
        }
        Err(error) => return Err(format!("cannot lock native build directory: {error}")),
    }
    Ok(file)
}

/// Units whose outputs `index` reads: its dependencies and build script, and
/// for a build script, the scripts whose `links` metadata it receives.
fn prerequisites(plan: &Plan, index: usize) -> Vec<usize> {
    let unit = &plan.units[index];
    let mut waits: Vec<usize> = unit.dependencies.iter().map(|(_, d)| *d).collect();
    waits.extend(unit.build_script);
    if unit.target.kind == "build-script"
        && let Some(owner) = plan.units.iter().find(|u| {
            u.package == unit.package
                && u.feature_domain == unit.feature_domain
                && u.target.kind != "build-script"
        })
    {
        waits.extend(
            owner
                .dependencies
                .iter()
                .filter_map(|(_, d)| linked_build_script(plan, *d).map(|(_, script)| script)),
        );
    }
    waits
}

/// Build scripts and native macro hosts compile their host libraries from
/// source. Before starting, wait for the outputs those libraries consume.
fn flux_host_prerequisites(plan: &Plan, root: usize, waits: &mut Vec<usize>) {
    let mut seen = BTreeSet::new();
    let mut pending: Vec<_> = plan.units[root]
        .dependencies
        .iter()
        .map(|(_, id)| *id)
        .collect();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let unit = &plan.units[id];
        waits.extend(unit.build_script);
        if unit.target.kind == "proc-macro" {
            waits.push(id);
        } else {
            pending.extend(unit.dependencies.iter().map(|(_, id)| *id));
        }
    }
}

/// A stable directory name for a unit: its identity, not its plan position
/// or the time of the run.
fn unit_name(plan: &Plan, index: usize, rustc: &str) -> String {
    let unit = &plan.units[index];
    let package = &plan.packages[unit.package];
    let identity = serde_json::json!([
        package.name,
        package.version,
        package.source,
        package.manifest,
        unit.domain,
        unit.feature_domain,
        unit.target.kind,
        unit.target.name,
        unit.target.root,
        unit.target.edition,
        unit.features,
        rustc,
    ]);
    let hash = hex(&crate::flib::digest(identity.to_string().as_bytes())[..8]);
    if unit.target.kind == "build-script" {
        format!("build-{}-{hash}", package.name)
    } else {
        format!("{}-{hash}", unit.target.name.replace('-', "_"))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Find scripts that can reuse a library built for another script in this plan.
fn shared_build_scripts(plan: &Plan) -> BTreeSet<usize> {
    let mut first_user = vec![None; plan.units.len()];
    let mut seen = vec![None; plan.units.len()];
    let mut shared = BTreeSet::new();
    let mut pending = Vec::new();
    let mut dependencies = Vec::new();
    for (script, unit) in plan.units.iter().enumerate() {
        if unit.domain != Domain::Host || unit.target.kind != "build-script" {
            continue;
        }
        pending.clear();
        dependencies.clear();
        pending.extend(unit.dependencies.iter().map(|(_, id)| *id));
        while let Some(id) = pending.pop() {
            if seen[id].replace(script) == Some(script) {
                continue;
            }
            // Proc-macro hosts have a separate compilation and loading path.
            if plan.units[id].target.kind == "proc-macro" {
                dependencies.clear();
                break;
            }
            dependencies.push(id);
            pending.extend(plan.units[id].dependencies.iter().map(|(_, dep)| *dep));
        }
        for &id in &dependencies {
            let first = *first_user[id].get_or_insert(script);
            if first != script {
                shared.insert(first);
                shared.insert(script);
            }
        }
    }
    shared
}

/// Settings shared by every host unit that Flux compiles.
struct HostSettings<'a> {
    version: &'a str,
    cfg: &'a str,
    rustc: &'a OsStr,
    native_macros: bool,
    debug_assertions: bool,
    release: bool,
    jobs: usize,
    compile_jobs: usize,
    optimize_scripts: bool,
    timings: bool,
    shared_dependencies: bool,
    artifacts: &'a FreshArtifacts,
    sdk: &'a OnceLock<Option<PathBuf>>,
}

/// Compiles a build script with Flux and runs it, or builds a Flux macro host.
fn flux_unit(
    plan: &Plan,
    outputs: &[Output],
    index: usize,
    root: &Path,
    host: &HostSettings<'_>,
) -> Result<Output> {
    let HostSettings {
        version,
        cfg,
        rustc,
        native_macros,
        debug_assertions,
        release,
        jobs,
        compile_jobs,
        optimize_scripts,
        timings,
        shared_dependencies,
        artifacts,
        sdk,
    } = *host;
    let unit = &plan.units[index];
    let package = &plan.packages[unit.package];
    let directory = root.join(unit_name(plan, index, version));
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let stem = unit.target.name.replace('-', "_");
    let mut output = Output::default();
    if unit.target.kind != "build-script" {
        let (sources, native) = host_sources(plan, outputs, index, version)?;
        if native != flux_codegen::NativeLink::default() {
            return Err(format!(
                "native libraries in the procedural macro host of {} are not supported yet",
                package.name
            ));
        }
        eprintln!("flux host {} proc-macro ({index})", package.name);
        output.macros = Some(crate::proc_macro_native::build_host_graph(
            sources, &directory,
        )?);
        output.artifact = Some(directory.join("host"));
        return Ok(output);
    }
    let mut env = environment(package, &unit.target)?;
    let built = unit.build_script.and_then(|id| outputs[id].script.as_ref());
    if let Some(built) = built {
        env.extend(built.environment.clone());
        env.insert(
            "OUT_DIR".into(),
            built.out_dir.to_string_lossy().into_owned(),
        );
    }
    let compile_started = timings.then(std::time::Instant::now);
    let artifact = directory.join("build-script");
    let (sources, native) = host_sources(plan, outputs, index, version)?;
    eprintln!("flux host {} build-script ({index})", package.name);
    let options = crate::BuildOptions {
        panic_strategy: flux_syntax::PanicStrategy::Abort,
        optimize: false,
        fast_build: false,
        optimize_build_script: optimize_scripts,
        timings,
        debug_assertions: Some(debug_assertions),
    };
    let compiled = if shared_dependencies && native == flux_codegen::NativeLink::default() {
        super::build::run_script(
            &sources,
            &artifact,
            &root.join("flux-deps"),
            artifacts,
            options,
            compile_jobs,
            &native,
        )? == ExitCode::SUCCESS
    } else {
        crate::build::compile_cached(
            CompileRequest {
                file: &unit.target.root,
                crate_name: &stem,
                output: &artifact,
                sources: &sources,
                native,
                options,
            },
            sdk,
        )?
    };
    if !compiled {
        return Err(format!(
            "cannot compile build script for {} with flux",
            package.name
        ));
    }
    if let Some(started) = compile_started {
        eprintln!(
            "build script {} compile: {:.2?}",
            package.name,
            started.elapsed()
        );
    }
    let out = directory.join("out");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let mut run = Command::new(&artifact);
    clean_environment(&mut run);
    run.current_dir(package.manifest.parent().ok_or("manifest has no parent")?)
        .envs(&env)
        .env("OUT_DIR", &out)
        .env("HOST", &plan.host)
        .env("TARGET", &plan.target)
        .env("OPT_LEVEL", if release { "3" } else { "0" })
        .env("DEBUG", if release { "false" } else { "true" })
        .env("PROFILE", if release { "release" } else { "debug" })
        .env("NUM_JOBS", jobs.to_string())
        .env(
            "RUSTC",
            if native_macros {
                std::env::current_exe()
                    .map_err(|e| e.to_string())?
                    .into_os_string()
            } else {
                rustc.to_owned()
            },
        )
        .env("CARGO_ENCODED_RUSTFLAGS", "");
    if native_macros {
        run.env("FLUX_COMPILER_PROBE", "1");
    }
    if let Some(links) = &package.links {
        run.env("CARGO_MANIFEST_LINKS", links);
    }
    for feature in &unit.features {
        run.env(format!("CARGO_FEATURE_{}", env_name(feature)?), "1");
    }
    for (name, value) in cfg_environment(cfg)? {
        run.env(name, value);
    }
    for (name, value) in dependency_metadata(plan, index, outputs)? {
        run.env(name, value);
    }
    let cache = script_cache::RunCache::new(&directory, &out);
    if let Some(stdout) = cache.fresh_stdout(&run)
        && let Ok((script, _)) = parse_script(&stdout, out.clone())
    {
        eprintln!("fresh build script {}", package.name);
        output.script = Some(script);
        output.artifact = Some(artifact);
        return Ok(output);
    }
    cache.invalidate().map_err(|e| e.to_string())?;
    let started = std::time::SystemTime::now();
    let run_started = timings.then(std::time::Instant::now);
    let result = run
        .output()
        .map_err(|e| format!("cannot run build script for {}: {e}", package.name))?;
    if let Some(started) = run_started {
        eprintln!(
            "build script {} run: {:.2?}",
            package.name,
            started.elapsed()
        );
    }
    std::fs::write(directory.join("stdout"), &result.stdout).map_err(|e| e.to_string())?;
    std::fs::write(directory.join("stderr"), &result.stderr).map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "build script for {} failed: {}\n{}\n{}",
            package.name,
            result.status,
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let text = String::from_utf8(result.stdout)
        .map_err(|e| format!("invalid build-script output: {e}"))?;
    let (script, rerun) = parse_script(&text, out.clone())?;
    cache.record(&run, &rerun, &text, started);
    output.script = Some(script);
    output.artifact = Some(artifact);
    Ok(output)
}

/// Profile settings for rustc-compiled host libraries and procedural macros.
struct RustcHostProfile {
    opt_level: String,
    debug_assertions: bool,
    overflow_checks: bool,
}

impl RustcHostProfile {
    fn read(manifest: &Path, release: bool) -> Result<Self> {
        let name = if release { "release" } else { "dev" };
        let env_prefix = format!("CARGO_PROFILE_{}", name.to_ascii_uppercase());
        let text = std::fs::read_to_string(manifest)
            .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
        let table: toml::Table = text
            .parse()
            .map_err(|e| format!("invalid manifest {}: {e}", manifest.display()))?;
        let profile = table.get("profile").and_then(|profiles| profiles.get(name));
        let overrides = profile.and_then(|profile| profile.get("build-override"));
        let env_setting = |prefix: &str, key: &str| {
            let env = format!("{prefix}_{}", key.replace('-', "_").to_ascii_uppercase());
            std::env::var_os(&env)
                .map(|value| {
                    value
                        .into_string()
                        .map_err(|_| format!("{env} must be Unicode"))
                })
                .transpose()
        };
        let env_override = |key: &str| env_setting(&format!("{env_prefix}_BUILD_OVERRIDE"), key);
        let opt_level = match (
            env_override("opt-level")?,
            overrides.and_then(|o| o.get("opt-level")),
        ) {
            (Some(value), _) => value,
            (None, None) => "0".into(),
            (None, Some(toml::Value::Integer(value))) => value.to_string(),
            (None, Some(toml::Value::String(value))) => value.clone(),
            _ => return Err(format!("invalid profile.{name}.build-override.opt-level")),
        };
        if !matches!(opt_level.as_str(), "0" | "1" | "2" | "3" | "s" | "z") {
            return Err(format!(
                "invalid profile.{name}.build-override.opt-level: {opt_level}"
            ));
        }
        // Unlike the optimization level, these settings inherit from the
        // parent profile. A rustc optimization flag alone also changes its
        // implicit assertion and overflow settings.
        let checks = |key: &str| {
            let error = || format!("invalid host profile {name} {key}");
            for (env, value) in [
                (env_override(key)?, overrides.and_then(|o| o.get(key))),
                (
                    env_setting(&env_prefix, key)?,
                    profile.and_then(|p| p.get(key)),
                ),
            ] {
                if let Some(env) = env {
                    return env.parse().map_err(|_| error());
                }
                if let Some(value) = value {
                    return value.as_bool().ok_or_else(error);
                }
            }
            Ok(!release)
        };
        Ok(Self {
            opt_level,
            debug_assertions: checks("debug-assertions")?,
            overflow_checks: checks("overflow-checks")?,
        })
    }

    fn apply(&self, command: &mut Command) {
        let optimized = self.opt_level != "0";
        if optimized {
            command.arg(format!("-Copt-level={}", self.opt_level));
        }
        if self.debug_assertions == optimized {
            command.arg(format!("-Cdebug-assertions={}", self.debug_assertions));
        }
        if self.overflow_checks != self.debug_assertions {
            command.arg(format!("-Coverflow-checks={}", self.overflow_checks));
        }
    }
}

/// A rustc compilation of one host library or proc macro, ready for a worker.
struct RustcUnit {
    command: Command,
    label: String,
    artifact: PathBuf,
    dep_info: PathBuf,
    fingerprint: PathBuf,
    key: String,
}

fn rustc_unit(
    plan: &Plan,
    outputs: &[Output],
    index: usize,
    root: &Path,
    rustc: &OsStr,
    version: &str,
    profile: &RustcHostProfile,
) -> Result<RustcUnit> {
    let unit = &plan.units[index];
    let package = &plan.packages[unit.package];
    let name = unit_name(plan, index, version);
    let directory = root.join(&name);
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let metadata = name.rsplit_once('-').map_or("", |(_, hash)| hash);
    let stem = unit.target.name.replace('-', "_");
    let deps = root.join("deps");
    let proc_macro = unit.target.kind == "proc-macro";
    let artifact = if proc_macro {
        deps.join(format!(
            "lib{stem}-{metadata}.{}",
            std::env::consts::DLL_EXTENSION
        ))
    } else {
        deps.join(format!("lib{stem}-{metadata}.rlib"))
    };
    let dep_info = directory.join("dep-info.d");
    let mut env = environment(package, &unit.target)?;
    let built = unit.build_script.and_then(|id| outputs[id].script.as_ref());
    if let Some(built) = built {
        env.extend(built.environment.clone());
        env.insert(
            "OUT_DIR".into(),
            built.out_dir.to_string_lossy().into_owned(),
        );
    }
    let mut command = Command::new(rustc);
    clean_environment(&mut command);
    command
        .current_dir(package.manifest.parent().ok_or("manifest has no parent")?)
        .envs(&env)
        .arg(&unit.target.root)
        .arg(format!("--edition={}", unit.target.edition))
        .args([
            "--crate-name",
            &stem,
            "--crate-type",
            if proc_macro { "proc-macro" } else { "rlib" },
        ])
        .arg(format!("-Cmetadata={metadata}"))
        // Rustc also writes intermediate object files, even when link and
        // dep-info below have explicit paths. Keep those out of the package
        // directory, which concurrent workspace builds may share.
        .arg("--out-dir")
        .arg(&directory)
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg(format!(
            "--emit=dep-info={},link={}",
            dep_info.display(),
            artifact.display()
        ));
    if package.source.is_some() {
        command.args(["--cap-lints", "allow"]);
    }
    profile.apply(&mut command);
    if proc_macro {
        command.args(["--extern", "proc_macro"]);
    }
    for feature in &unit.features {
        command.arg("--cfg").arg(format!("feature={feature:?}"));
    }
    if let Some(built) = built {
        for cfg in &built.cfgs {
            command.arg("--cfg").arg(cfg);
        }
        for cfg in &built.check_cfgs {
            command.arg("--check-cfg").arg(cfg);
        }
        for (flag, value) in &built.linker {
            command.arg(flag).arg(value);
        }
    }
    // Inputs: the compiler, the exact command and the dependencies' own
    // fingerprints, which change whenever a dependency is rebuilt.
    // Variables the command only removes are left out: which of them exist
    // depends on the caller's environment, and dep-info records any read.
    let mut key = format!(
        "{version}\n{:?}\n{:?}\n",
        command.get_program(),
        command.get_current_dir()
    );
    for arg in command.get_args() {
        key.push_str(&format!("{arg:?}\n"));
    }
    for (name, value) in command.get_envs() {
        if let Some(value) = value {
            key.push_str(&format!("{name:?}={value:?}\n"));
        }
    }
    for (alias, dependency) in &unit.dependencies {
        let path = outputs[*dependency]
            .artifact
            .as_ref()
            .ok_or("missing compiled host dependency")?;
        command
            .arg("--extern")
            .arg(format!("{alias}={}", path.display()));
        let fingerprint = std::fs::read(fingerprint_path(path)).unwrap_or_default();
        key.push_str(&format!(
            "{alias}={} {}\n",
            path.display(),
            hex(&crate::flib::digest(&fingerprint))
        ));
    }
    Ok(RustcUnit {
        command,
        label: format!("{} {} ({index})", package.name, unit.target.kind),
        fingerprint: fingerprint_path(&artifact),
        artifact,
        dep_info,
        key: hex(&crate::flib::digest(key.as_bytes())),
    })
}

/// Recorded beside the artifact, so that dependents can find it.
fn fingerprint_path(artifact: &Path) -> PathBuf {
    artifact.with_extension("fingerprint")
}

/// What a fresh rustc artifact was built from: the command's key, and every
/// source file and environment variable rustc's dep-info reported reading.
#[derive(Serialize, serde::Deserialize, PartialEq, Eq)]
struct Fingerprint {
    key: String,
    files: Vec<(PathBuf, String)>,
    env: Vec<(String, Option<String>)>,
}

impl RustcUnit {
    fn run(mut self) -> Result<()> {
        if self.fresh() {
            eprintln!("fresh host {}", self.label);
            return Ok(());
        }
        // A failed or interrupted build must not look fresh afterwards.
        let _ = std::fs::remove_file(&self.fingerprint);
        eprintln!("rustc host {}", self.label);
        crate::cargo::output(&mut self.command, "native host compilation")?;
        let fingerprint = self.record()?;
        std::fs::write(
            &self.fingerprint,
            serde_json::to_vec(&fingerprint).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }

    fn fresh(&self) -> bool {
        let Ok(bytes) = std::fs::read(&self.fingerprint) else {
            return false;
        };
        let Ok(recorded) = serde_json::from_slice::<Fingerprint>(&bytes) else {
            return false;
        };
        recorded.key == self.key
            && self.artifact.is_file()
            && recorded.files.iter().all(|(path, hash)| {
                std::fs::read(path).is_ok_and(|b| hex(&crate::flib::digest(&b)) == *hash)
            })
            && recorded
                .env
                .iter()
                .all(|(name, value)| self.env_value(name) == *value)
    }

    /// The value rustc sees for `name`: set or removed by the command, or inherited.
    fn env_value(&self, name: &str) -> Option<String> {
        match self.command.get_envs().find(|(key, _)| *key == name) {
            Some((_, value)) => value.map(|v| v.to_string_lossy().into_owned()),
            None => std::env::var(name).ok(),
        }
    }

    fn record(&self) -> Result<Fingerprint> {
        let text = std::fs::read_to_string(&self.dep_info).map_err(|e| e.to_string())?;
        let directory = self.command.get_current_dir().unwrap_or(Path::new("."));
        let mut fingerprint = Fingerprint {
            key: self.key.clone(),
            files: Vec::new(),
            env: Vec::new(),
        };
        for line in text.lines() {
            if let Some(variable) = line.strip_prefix("# env-dep:") {
                let (name, value) = match variable.split_once('=') {
                    Some((name, value)) => (name, Some(unescape_env(value))),
                    None => (variable, None),
                };
                fingerprint.env.push((name.to_owned(), value));
            } else if let Some(path) = line.strip_suffix(':')
                && !path.is_empty()
                && !path.starts_with('#')
            {
                // Empty make rules name each input file once.
                let path = directory.join(path.replace("\\ ", " "));
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                fingerprint
                    .files
                    .push((path, hex(&crate::flib::digest(&bytes))));
            }
        }
        Ok(fingerprint)
    }
}

fn unescape_env(value: &str) -> String {
    let mut text = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match (c, chars.clone().next()) {
            ('\\', Some('n')) => {
                text.push('\n');
                chars.next();
            }
            ('\\', Some('\\')) => {
                text.push('\\');
                chars.next();
            }
            _ => text.push(c),
        }
    }
    text
}

// Only libraries consumed by a rustc ABI proc-macro host need rustc artifacts.
fn rustc_dependencies(plan: &Plan, id: usize, needed: &mut BTreeSet<usize>) {
    if !needed.insert(id) {
        return;
    }
    for (_, dependency) in &plan.units[id].dependencies {
        rustc_dependencies(plan, *dependency, needed);
    }
}

/// The source graph of a host unit, and the native libraries its
/// dependencies' build scripts request.
fn host_sources(
    plan: &Plan,
    outputs: &[Output],
    root: usize,
    rustc: &str,
) -> Result<(Vec<SourceInput>, flux_codegen::NativeLink)> {
    #[allow(clippy::too_many_arguments)]
    fn visit(
        plan: &Plan,
        outputs: &[Output],
        id: usize,
        rustc: &str,
        root: usize,
        indices: &mut BTreeMap<usize, usize>,
        sources: &mut Vec<SourceInput>,
        native: &mut flux_codegen::NativeLink,
    ) -> Result<usize> {
        if let Some(index) = indices.get(&id) {
            return Ok(*index);
        }
        let unit = &plan.units[id];
        let script = unit.build_script.and_then(|i| outputs[i].script.as_ref());
        let mut cfg = crate::cargo::cfg(script.map_or(&[], |s| s.cfgs.as_slice()), &unit.features)?;
        let mut environment = environment(&plan.packages[unit.package], &unit.target)?;
        if let Some(script) = script {
            // A dependency proc macro runs in its own host, outside this link.
            if unit.target.kind != "proc-macro" || id == root {
                link_directives(plan, id, script, native)?;
            }
            environment.extend(script.environment.clone());
            environment.insert(
                "OUT_DIR".into(),
                script.out_dir.to_string_lossy().into_owned(),
            );
        }
        let macros = if unit.target.kind == "proc-macro" && id != root {
            cfg.names.push(flux_intern::Symbol::intern("proc_macro"));
            Some(macro_exports(
                rustc,
                &outputs[id],
                &unit.target,
                cfg.clone(),
                None,
            )?)
        } else {
            None
        };
        let mut dependencies = Vec::new();
        if macros.is_none() {
            for (alias, dependency) in &unit.dependencies {
                dependencies.push((
                    alias.clone(),
                    visit(
                        plan,
                        outputs,
                        *dependency,
                        rustc,
                        root,
                        indices,
                        sources,
                        native,
                    )?,
                ));
            }
        }
        let index = sources.len();
        indices.insert(id, index);
        sources.push(SourceInput {
            compilation_dir: Some(plan.workspace_root.clone()),
            package_identity: None,
            host_macros: macros,
            file: unit.target.root.clone(),
            name: unit.target.name.replace('-', "_"),
            edition: crate::cargo::edition(&unit.target.edition)?,
            dependencies,
            cfg,
            environment: Some(environment),
        });
        Ok(index)
    }
    let mut sources = Vec::new();
    let mut native = flux_codegen::NativeLink::default();
    visit(
        plan,
        outputs,
        root,
        rustc,
        root,
        &mut BTreeMap::new(),
        &mut sources,
        &mut native,
    )?;
    Ok((sources, native))
}

/// Adds the `-l` and `-L` requests of `unit`'s build script to `native`.
/// Like Cargo, every library's requests reach each link that includes it.
fn link_directives(
    plan: &Plan,
    unit: usize,
    script: &ScriptOutput,
    native: &mut flux_codegen::NativeLink,
) -> Result<()> {
    let directory = package_directory(plan, unit)?;
    for (flag, value) in &script.linker {
        if flag == "-l" {
            crate::cargo::link_library(native, value)?;
        } else {
            crate::cargo::link_search(native, value, directory);
        }
    }
    Ok(())
}

fn package_directory(plan: &Plan, unit: usize) -> Result<&Path> {
    let manifest = &plan.packages[plan.units[unit].package].manifest;
    Ok(manifest.parent().ok_or("manifest has no parent")?)
}

fn macro_exports(
    rustc: &str,
    output: &Output,
    target: &Target,
    cfg: flux_hir::CrateCfg,
    server: Option<&Path>,
) -> Result<Vec<flux_hir::ProcMacro>> {
    if let Some(exports) = &output.macros {
        return Ok(exports.clone());
    }
    let artifact = output.artifact.as_ref().ok_or("missing host macro")?;
    let discovered;
    let server = if let Some(server) = server {
        server
    } else {
        discovered = crate::proc_macro::rustc_server(rustc)?;
        &discovered
    };
    crate::proc_macro::exports_from_server(
        server,
        artifact,
        &target.root,
        crate::cargo::edition(&target.edition)?,
        cfg,
        Some(&artifact.with_extension("macro-exports-v1")),
    )
}

fn environment(package: &PlannedPackage, target: &Target) -> Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::from([
        (
            "CARGO_MANIFEST_DIR".into(),
            package
                .manifest
                .parent()
                .ok_or("manifest has no parent")?
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "CARGO_MANIFEST_PATH".into(),
            package.manifest.to_string_lossy().into_owned(),
        ),
        ("CARGO_PKG_NAME".into(), package.name.clone()),
        ("CARGO_PKG_VERSION".into(), package.version.clone()),
        ("CARGO_CRATE_NAME".into(), target.name.replace('-', "_")),
    ]);
    let version = semver::Version::parse(&package.version).map_err(|e| e.to_string())?;
    for (key, value) in [
        ("MAJOR", version.major.to_string()),
        ("MINOR", version.minor.to_string()),
        ("PATCH", version.patch.to_string()),
        ("PRE", version.pre.to_string()),
    ] {
        env.insert(format!("CARGO_PKG_VERSION_{key}"), value);
    }
    for (key, field) in [
        ("DESCRIPTION", "description"),
        ("HOMEPAGE", "homepage"),
        ("REPOSITORY", "repository"),
        ("LICENSE", "license"),
        ("LICENSE_FILE", "license-file"),
        ("RUST_VERSION", "rust-version"),
    ] {
        env.insert(
            format!("CARGO_PKG_{key}"),
            package
                .fields
                .get(field)
                .and_then(toml::Value::as_str)
                .unwrap_or("")
                .into(),
        );
    }
    env.insert(
        "CARGO_PKG_AUTHORS".into(),
        super::strings(package.fields.get("authors"))?.join(":"),
    );
    if matches!(target.kind.as_str(), "bin" | "test-bin") {
        env.insert("CARGO_BIN_NAME".into(), target.name.clone());
    }
    Ok(env)
}
fn clean_environment(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        let text = key.to_string_lossy();
        if text.starts_with("CARGO_PKG_")
            || text.starts_with("CARGO_FEATURE_")
            || text.starts_with("CARGO_CFG_")
            || text.starts_with("DEP_")
            || matches!(
                text.as_ref(),
                "OUT_DIR"
                    | "CARGO_CRATE_NAME"
                    | "CARGO_BIN_NAME"
                    | "CARGO_MANIFEST_DIR"
                    | "CARGO_MANIFEST_PATH"
                    | "CARGO_MANIFEST_LINKS"
                    | "HOST"
                    | "TARGET"
                    | "PROFILE"
                    | "OPT_LEVEL"
                    | "DEBUG"
                    | "NUM_JOBS"
            )
        {
            command.env_remove(key);
        }
    }
}
fn env_name(name: &str) -> Result<String> {
    if !name.is_ascii() {
        return Err("non-ascii build-script environment names are not supported yet".into());
    }
    Ok(name
        .chars()
        .map(|c| {
            if c == '-' {
                '_'
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect())
}
fn cfg_environment(cfg: &str) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::<String, Vec<String>>::new();
    for line in cfg.lines() {
        let (key, value) = if let Some((key, value)) = line.split_once('=') {
            (
                key,
                serde_json::from_str::<String>(value).map_err(|e| e.to_string())?,
            )
        } else {
            (line, String::new())
        };
        values
            .entry(format!("CARGO_CFG_{}", env_name(key)?))
            .or_default()
            .push(value);
    }
    Ok(values.into_iter().map(|(k, v)| (k, v.join(","))).collect())
}
fn dependency_metadata(
    plan: &Plan,
    script: usize,
    outputs: &[Output],
) -> Result<BTreeMap<String, String>> {
    let unit = &plan.units[script];
    // A build script receives links metadata from normal dependencies of its package,
    // not from the libraries used to compile the build script itself.
    let owner = plan
        .units
        .iter()
        .find(|u| {
            u.package == unit.package
                && u.feature_domain == unit.feature_domain
                && u.target.kind != "build-script"
        })
        .ok_or("build script has no owning package unit")?;
    let mut env = BTreeMap::new();
    for (links, script) in owner
        .dependencies
        .iter()
        .filter_map(|(_, d)| linked_build_script(plan, *d))
    {
        let output = outputs
            .get(script)
            .and_then(|o| o.script.as_ref())
            .ok_or("dependency metadata is not ready")?;
        for (key, value) in &output.metadata {
            env.insert(
                format!("DEP_{}_{}", env_name(links)?, env_name(key)?),
                value.clone(),
            );
        }
    }
    Ok(env)
}
fn linked_build_script(plan: &Plan, dependency: usize) -> Option<(&str, usize)> {
    let unit = &plan.units[dependency];
    Some((
        plan.packages[unit.package].links.as_deref()?,
        unit.build_script?,
    ))
}

fn parse_script(text: &str, out_dir: PathBuf) -> Result<(ScriptOutput, script_cache::Rerun)> {
    let mut output = ScriptOutput {
        out_dir,
        ..Default::default()
    };
    let mut rerun = script_cache::Rerun::default();
    for line in text.lines() {
        let (instruction, modern) = if let Some(s) = line.strip_prefix("cargo::") {
            (s, true)
        } else if let Some(s) = line.strip_prefix("cargo:") {
            (s, false)
        } else {
            continue;
        };
        let (key, value) = instruction
            .split_once('=')
            .ok_or_else(|| format!("invalid build-script directive: {line}"))?;
        match key {
            "rerun-if-changed" => {
                rerun.paths.insert(value.into());
            }
            "rerun-if-env-changed" => {
                rerun.environment.insert(value.into());
            }
            "rustc-cfg" => {
                crate::cargo::cfg(&[value.into()], &[])?;
                output.cfgs.push(value.into());
            }
            "rustc-check-cfg" => output.check_cfgs.push(value.into()),
            "rustc-env" => {
                let (key, value) = value
                    .split_once('=')
                    .ok_or("expected rustc-env=NAME=VALUE")?;
                output.environment.insert(key.into(), value.into());
            }
            "rustc-link-lib" => output.linker.push(("-l".into(), value.into())),
            "rustc-link-arg" | "rustc-link-arg-bins" | "rustc-link-arg-bin" => {
                output
                    .link_args
                    .extend(crate::cargo::binary_link_arg(key, value)?);
            }
            // Flux builds only binaries.
            "rustc-link-arg-tests"
            | "rustc-link-arg-examples"
            | "rustc-link-arg-benches"
            | "rustc-link-arg-cdylib"
            | "rustc-cdylib-link-arg" => {}
            "rustc-link-search" => output.linker.push(("-L".into(), value.into())),
            "rustc-flags" => {
                let mut flags = value.split_whitespace();
                while let Some(flag) = flags.next() {
                    if matches!(flag, "-l" | "-L") {
                        output.linker.push((
                            flag.into(),
                            flags.next().ok_or("missing rustc-flags argument")?.into(),
                        ));
                    } else if flag.starts_with("-l") || flag.starts_with("-L") {
                        output.linker.push((flag[..2].into(), flag[2..].into()));
                    } else {
                        return Err(format!("unsupported build-script rustc flag {flag}"));
                    }
                }
            }
            "metadata" if modern => {
                let (key, value) = value.split_once('=').ok_or("expected metadata=KEY=VALUE")?;
                output.metadata.insert(key.into(), value.into());
            }
            "warning" => eprintln!("build script warning: {value}"),
            "error" => return Err(format!("build script error: {value}")),
            _ if key.starts_with("rustc-") || modern => {
                return Err(format!("unsupported build-script directive {key}"));
            }
            _ => {
                output.metadata.insert(key.into(), value.into());
            }
        }
    }
    Ok((output, rerun))
}

fn compile_plan(
    prepared: &Prepared,
    options: crate::BuildOptions,
    jobs: usize,
    cli: &super::Options,
    macro_server: &OnceLock<Result<PathBuf>>,
    early_libraries: &BTreeMap<PathBuf, PathBuf>,
) -> Result<ExitCode> {
    let mut failed = false;
    for &root in &prepared.plan.root_units {
        let status = compile_root(prepared, root, options, jobs, macro_server, early_libraries)?;
        if status != ExitCode::SUCCESS {
            return Ok(status);
        }
        if cli.test_mode
            && prepared.plan.units[root].target.kind.starts_with("test")
            && !cli.test_no_run
        {
            let output = prepared
                .plan
                .workspace_root
                .join("target/flux")
                .join(options.profile_name())
                .join(format!(
                    "{}-{}",
                    prepared.plan.units[root].target.kind, prepared.plan.units[root].target.name
                ));
            let status = Command::new(output)
                .args(&cli.test_args)
                .current_dir(package_directory(&prepared.plan, root)?)
                .status()
                .map_err(|e| e.to_string())?;
            failed |= !status.success();
        }
    }
    Ok(if failed {
        ExitCode::from(101)
    } else {
        ExitCode::SUCCESS
    })
}

type MacroCatalog = (flux_hir::CrateCfg, Vec<flux_hir::ProcMacro>);

fn target_macro_catalog(
    prepared: &Prepared,
    id: usize,
    macro_server: &OnceLock<Result<PathBuf>>,
) -> Result<MacroCatalog> {
    let unit = &prepared.plan.units[id];
    let script = unit
        .build_script
        .and_then(|id| prepared.outputs[id].script.as_ref());
    let mut cfg = crate::cargo::cfg(script.map_or(&[], |s| s.cfgs.as_slice()), &unit.features)?;
    cfg.names.push(flux_intern::Symbol::intern("proc_macro"));
    let output = &prepared.outputs[id];
    let server = if output.macros.is_none() {
        Some(
            macro_server
                .get_or_init(|| crate::proc_macro::rustc_server(&prepared.rustc))
                .as_ref()
                .map_err(Clone::clone)?
                .as_path(),
        )
    } else {
        None
    };
    let exports = macro_exports(&prepared.rustc, output, &unit.target, cfg.clone(), server)?;
    Ok((cfg, exports))
}

fn prepare_macro_catalogs(
    prepared: &Prepared,
    units: &[usize],
    jobs: usize,
    macro_server: &OnceLock<Result<PathBuf>>,
) -> BTreeMap<usize, Result<MacroCatalog>> {
    if jobs < 2 || units.len() < 2 {
        return BTreeMap::new();
    }
    let prepare = |batch: &[usize]| -> Vec<_> {
        batch
            .iter()
            .map(|&id| (id, target_macro_catalog(prepared, id, macro_server)))
            .collect()
    };
    std::thread::scope(|scope| {
        let mut batches = units.chunks(units.len().div_ceil(jobs));
        let first = batches.next().expect("macro catalog work");
        let mut workers = Vec::new();
        let mut catalogs = BTreeMap::new();
        for batch in batches {
            match std::thread::Builder::new()
                .stack_size(8 * 1024 * 1024)
                .spawn_scoped(scope, move || prepare(batch))
            {
                Ok(worker) => workers.push(worker),
                Err(_) => catalogs.extend(prepare(batch)),
            }
        }
        catalogs.extend(prepare(first));
        for worker in workers {
            catalogs.extend(worker.join().expect("macro catalog worker panicked"));
        }
        catalogs
    })
}

fn target_source(
    plan: &Plan,
    id: usize,
    script: Option<&ScriptOutput>,
    cfg: flux_hir::CrateCfg,
    macros: Option<Vec<flux_hir::ProcMacro>>,
    file: PathBuf,
    dependencies: Vec<(String, usize)>,
) -> Result<SourceInput> {
    let unit = &plan.units[id];
    let mut env = environment(&plan.packages[unit.package], &unit.target)?;
    if let Some(script) = script {
        env.extend(script.environment.clone());
        env.insert(
            "OUT_DIR".into(),
            script.out_dir.to_string_lossy().into_owned(),
        );
    }
    Ok(SourceInput {
        compilation_dir: Some(plan.workspace_root.clone()),
        package_identity: None,
        host_macros: macros,
        file,
        name: unit.target.name.replace('-', "_"),
        edition: crate::cargo::edition(&unit.target.edition)?,
        dependencies,
        cfg,
        environment: Some(env),
    })
}

fn compile_root(
    prepared: &Prepared,
    root_unit: usize,
    options: crate::BuildOptions,
    jobs: usize,
    macro_server: &OnceLock<Result<PathBuf>>,
    early_libraries: &BTreeMap<PathBuf, PathBuf>,
) -> Result<ExitCode> {
    let plan = &prepared.plan;
    if plan.units[root_unit].target.kind == "proc-macro" {
        return Ok(ExitCode::SUCCESS);
    }
    let mut needed = BTreeSet::new();
    let mut pending = vec![root_unit];
    while let Some(id) = pending.pop() {
        if needed.insert(id) {
            pending.extend(prerequisites(plan, id));
        }
    }
    let target_imports: BTreeSet<_> = plan
        .units
        .iter()
        .filter(|unit| unit.domain == Domain::Target)
        .flat_map(|unit| unit.dependencies.iter().map(|(_, id)| *id))
        .filter(|&id| plan.units[id].target.kind == "proc-macro")
        .collect();
    let macros: Vec<_> = needed
        .iter()
        .copied()
        .filter(|&id| {
            plan.units[id].target.kind == "proc-macro"
                && (plan.units[id].domain == Domain::Target || target_imports.contains(&id))
                && prepared.outputs[id].macros.is_none()
        })
        .collect();
    let mut catalogs = prepare_macro_catalogs(prepared, &macros, jobs, macro_server);
    let mut sources = Vec::new();
    let mut indices = BTreeMap::new();
    let mut native = flux_codegen::NativeLink::default();
    let mut doc_dependencies = None;
    for (id, unit) in plan.units.iter().enumerate() {
        if !needed.contains(&id) {
            continue;
        }
        if unit.target.kind == "build-script"
            || (unit.domain == Domain::Host && unit.target.kind != "proc-macro")
        {
            continue;
        }
        // Only proc macros imported by target units need a Flux host catalog.
        if unit.domain == Domain::Host && !target_imports.contains(&id) {
            continue;
        }
        let script = unit
            .build_script
            .and_then(|i| prepared.outputs[i].script.as_ref());
        let (cfg, macros) = if unit.target.kind == "proc-macro" {
            let (cfg, exports) = catalogs
                .remove(&id)
                .unwrap_or_else(|| target_macro_catalog(prepared, id, macro_server))?;
            (cfg, Some(exports))
        } else {
            let cfg = crate::cargo::cfg(script.map_or(&[], |s| s.cfgs.as_slice()), &unit.features)?;
            (cfg, None)
        };
        if macros.is_none()
            && let Some(script) = script
        {
            link_directives(plan, id, script, &mut native)?;
        }
        let dependencies = if macros.is_some() {
            Vec::new()
        } else {
            unit.dependencies
                .iter()
                .map(|(alias, id)| {
                    indices
                        .get(id)
                        .copied()
                        .map(|index| (alias.clone(), index))
                        .ok_or("missing target dependency".to_owned())
                })
                .collect::<Result<Vec<_>>>()?
        };
        if unit.target.kind == "doctest" {
            doc_dependencies = Some(dependencies);
            continue;
        }
        let file = if macros.is_some() {
            prepared.outputs[id]
                .artifact
                .clone()
                .ok_or("missing host macro")?
        } else {
            unit.target.root.clone()
        };
        indices.insert(id, sources.len());
        sources.push(target_source(
            plan,
            id,
            script,
            cfg,
            macros,
            file,
            dependencies,
        )?);
    }
    if let Some(dependencies) = doc_dependencies {
        let binary = &plan.units[root_unit];
        let root = dependencies
            .iter()
            .find_map(|(_, index)| (sources[*index].file == binary.target.root).then_some(*index))
            .ok_or("missing doctest library")?;
        let output = plan
            .workspace_root
            .join("target/flux")
            .join(options.profile_name())
            .join(format!("lib{}.flib", sources[root].name));
        std::fs::create_dir_all(output.parent().ok_or("doctest output has no parent")?)
            .map_err(|e| e.to_string())?;
        let status = super::build::run(&sources, root, &output, options, jobs, true, &native)?;
        if status != ExitCode::SUCCESS {
            return Ok(status);
        }
        // A dependency error must never satisfy an example's compile_fail flag.
        // Check every direct doctest dependency before interpreting examples.
        for (alias, index) in &dependencies {
            if *index == root {
                continue;
            }
            let checked = output.with_file_name(format!("lib{alias}-doctest-dependency.flib"));
            let status =
                super::build::run(&sources, *index, &checked, options, jobs, true, &native)?;
            if status != ExitCode::SUCCESS {
                return Ok(status);
            }
        }
        return crate::doctest::run(
            &sources,
            root,
            &dependencies,
            &output,
            options,
            jobs,
            &native,
        );
    }
    let root = *indices.get(&root_unit).ok_or("missing root unit")?;
    // Link arguments apply to the package's own binaries only.
    let binary = &plan.units[root_unit];
    let test = binary.target.kind.starts_with("test");
    if test {
        sources[root].cfg.names.extend([
            flux_intern::Symbol::intern("test"),
            flux_intern::Symbol::intern("__flux_test_harness"),
        ]);
    }
    let library = !matches!(
        binary.target.kind.as_str(),
        "bin" | "test" | "test-lib" | "test-bin"
    );
    if let Some(script) = (binary.build_script).and_then(|i| prepared.outputs[i].script.as_ref()) {
        let arguments: Vec<String> = (script.link_args.iter())
            .filter(|(name, _)| name.as_ref().is_none_or(|n| *n == binary.target.name))
            .map(|(_, argument)| argument.clone())
            .collect();
        crate::cargo::link_arguments(&mut native, &arguments, package_directory(plan, root_unit)?)?;
    }
    let output = plan
        .workspace_root
        .join("target/flux")
        .join(options.profile_name())
        .join(if library {
            format!("lib{}.flib", binary.target.name.replace('-', "_"))
        } else if test {
            format!("{}-{}", binary.target.kind, binary.target.name)
        } else {
            binary.target.name.clone()
        });
    std::fs::create_dir_all(output.parent().ok_or("build output has no parent")?)
        .map_err(|e| e.to_string())?;
    super::build::run_with_early_libraries(
        &sources,
        root,
        &output,
        options,
        jobs,
        library,
        &native,
        early_libraries,
    )
}
