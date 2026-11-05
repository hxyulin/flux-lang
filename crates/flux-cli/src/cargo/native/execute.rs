//! Flux build-script compilation and native host preparation.
use super::{Domain, Options, Plan, PlannedPackage, Result, Target};
use crate::build::{CompileRequest, SourceInput};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Default, Serialize)]
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
#[derive(Default, Serialize)]
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
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let version = if !rustc_units.is_empty() {
        crate::cargo::output(Command::new(&rustc).arg("-vV"), "rustc version")?
    } else {
        String::new()
    };
    // Build scripts and the target graph follow the Cargo profile: `release`
    // with `-O`, `dev` otherwise. An explicit setting overrides it.
    let optimize = build.is_some_and(|b| b.optimize);
    let debug_assertions = match build.and_then(|b| b.debug_assertions) {
        Some(enabled) => enabled,
        None => crate::cargo::profile_debug_assertions(
            &plan.workspace_root.join("Cargo.toml"),
            optimize,
        )?,
    };
    let build = build.map(|b| crate::BuildOptions {
        debug_assertions: Some(debug_assertions),
        ..b
    });
    let cfg = crate::target::cfg(debug_assertions)?
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
    let waits: Vec<Vec<usize>> = (0..plan.units.len())
        .map(|index| prerequisites(&plan, index))
        .collect();
    let mut outputs: Vec<Output> = plan.units.iter().map(|_| Output::default()).collect();
    let mut done: Vec<bool> = selected.iter().map(|s| !s).collect();
    let mut started = done.clone();
    let jobs = std::thread::available_parallelism().map_or(1, |n| n.get());
    let (finished, completions) = std::sync::mpsc::channel::<(usize, Result<()>)>();
    let mut running = 0;
    let mut failure = None;
    loop {
        while let Ok((index, result)) = completions.try_recv() {
            running -= 1;
            match result {
                Ok(()) => done[index] = true,
                Err(error) => {
                    failure.get_or_insert(error);
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
                started[index] = true;
                let work = match rustc_unit(&plan, &outputs, index, &root, &rustc, &version) {
                    Ok(work) => work,
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                };
                outputs[index].artifact = Some(work.artifact.clone());
                running += 1;
                let finished = finished.clone();
                std::thread::spawn(move || {
                    let _ = finished.send((index, work.run()));
                });
            }
            // Flux compiles build scripts and native macro hosts on this
            // thread, while rustc units keep running in their workers.
            let next =
                (0..plan.units.len()).find(|&i| ready(&started, i) && (script(i) || native_macros));
            if let Some(index) = next.filter(|_| failure.is_none()) {
                started[index] = true;
                let host = HostSettings {
                    version: &version,
                    cfg: &cfg,
                    rustc: &rustc,
                    native_macros,
                    debug_assertions,
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
        }
        if running == 0 {
            break;
        }
        let (index, result) = completions.recv().map_err(|e| e.to_string())?;
        running -= 1;
        match result {
            Ok(()) => done[index] = true,
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
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
            if let Some(start) = start {
                eprintln!(
                    "{:<38}  {:>10.2?}",
                    "planning + native host preparation",
                    start.elapsed()
                );
            }
            compile_plan(&prepared, options)
        }
        None => Ok(ExitCode::SUCCESS),
    }
}

/// Serializes preparations of one workspace, which share its host directory.
fn lock_directory(root: &Path) -> Result<std::fs::File> {
    let file = std::fs::File::create(root.join(".lock")).map_err(|e| e.to_string())?;
    if let Err(std::fs::TryLockError::WouldBlock) = file.try_lock() {
        eprintln!(
            "waiting for another native host preparation in {}",
            root.display()
        );
        file.lock().map_err(|e| e.to_string())?;
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
                .filter_map(|(_, d)| plan.units[*d].build_script),
        );
    }
    waits
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
    let hash = hex(&flux_hir::flib::digest(identity.to_string().as_bytes())[..8]);
    if unit.target.kind == "build-script" {
        format!("build-{}-{hash}", package.name)
    } else {
        format!("{}-{hash}", unit.target.name.replace('-', "_"))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Settings shared by every host unit that Flux compiles.
struct HostSettings<'a> {
    version: &'a str,
    cfg: &'a str,
    rustc: &'a std::ffi::OsStr,
    native_macros: bool,
    debug_assertions: bool,
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
    let artifact = directory.join("build-script");
    let (sources, native) = host_sources(plan, outputs, index, version)?;
    eprintln!("flux host {} build-script ({index})", package.name);
    if !crate::build::compile(CompileRequest {
        file: &unit.target.root,
        crate_name: &stem,
        output: &artifact,
        sources: &sources,
        native,
        options: crate::BuildOptions {
            optimize: false,
            timings: false,
            debug_assertions: Some(debug_assertions),
        },
    })? {
        return Err(format!(
            "cannot compile build script for {} with flux",
            package.name
        ));
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
        .env("OPT_LEVEL", "0")
        .env("DEBUG", "true")
        .env("PROFILE", "debug")
        .env("NUM_JOBS", "1")
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
    let result = run
        .output()
        .map_err(|e| format!("cannot run build script for {}: {e}", package.name))?;
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
    output.script = Some(parse_script(&text, out)?);
    output.artifact = Some(artifact);
    Ok(output)
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
    rustc: &std::ffi::OsStr,
    version: &str,
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
            hex(&flux_hir::flib::digest(&fingerprint))
        ));
    }
    Ok(RustcUnit {
        command,
        label: format!("{} {} ({index})", package.name, unit.target.kind),
        fingerprint: fingerprint_path(&artifact),
        artifact,
        dep_info,
        key: hex(&flux_hir::flib::digest(key.as_bytes())),
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
                std::fs::read(path).is_ok_and(|b| hex(&flux_hir::flib::digest(&b)) == *hash)
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
                    .push((path, hex(&flux_hir::flib::digest(&bytes))));
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
                &unit.target.root,
                cfg.clone(),
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
    source: &std::path::Path,
    cfg: flux_hir::CrateCfg,
) -> Result<Vec<flux_hir::ProcMacro>> {
    if let Some(exports) = &output.macros {
        return Ok(exports.clone());
    }
    crate::proc_macro::exports(
        rustc,
        output.artifact.as_ref().ok_or("missing host macro")?,
        source,
        cfg,
    )
}

fn environment(package: &PlannedPackage, target: &Target) -> Result<BTreeMap<String, String>> {
    let manifest = super::load(&package.manifest)?;
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
            manifest
                .fields
                .get(field)
                .and_then(toml::Value::as_str)
                .unwrap_or("")
                .into(),
        );
    }
    env.insert(
        "CARGO_PKG_AUTHORS".into(),
        super::strings(manifest.fields.get("authors"))?.join(":"),
    );
    if target.kind == "bin" {
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
    for (_, dependency) in &owner.dependencies {
        let dependency = &plan.units[*dependency];
        if let Some(links) = &plan.packages[dependency.package].links
            && let Some(script) = dependency.build_script
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
    }
    Ok(env)
}
fn parse_script(text: &str, out_dir: PathBuf) -> Result<ScriptOutput> {
    let mut output = ScriptOutput {
        out_dir,
        ..Default::default()
    };
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
            "rerun-if-changed" | "rerun-if-env-changed" => {} // Every preparation reruns all host units.
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
    Ok(output)
}

fn compile_plan(prepared: &Prepared, options: crate::BuildOptions) -> Result<ExitCode> {
    let plan = &prepared.plan;
    let mut sources = Vec::new();
    let mut indices = BTreeMap::new();
    let mut native = flux_codegen::NativeLink::default();
    for (id, unit) in plan.units.iter().enumerate() {
        if unit.target.kind == "build-script"
            || (unit.domain == Domain::Host && unit.target.kind != "proc-macro")
        {
            continue;
        }
        // Only proc macros imported by target units need a Flux host catalog.
        if unit.domain == Domain::Host
            && !plan
                .units
                .iter()
                .any(|u| u.domain == Domain::Target && u.dependencies.iter().any(|(_, d)| *d == id))
        {
            continue;
        }
        let script = unit
            .build_script
            .and_then(|i| prepared.outputs[i].script.as_ref());
        let mut cfg = crate::cargo::cfg(script.map_or(&[], |s| s.cfgs.as_slice()), &unit.features)?;
        let macros = if unit.target.kind == "proc-macro" {
            cfg.names.push(flux_intern::Symbol::intern("proc_macro"));
            Some(macro_exports(
                &prepared.rustc,
                &prepared.outputs[id],
                &unit.target.root,
                cfg.clone(),
            )?)
        } else {
            None
        };
        if macros.is_none()
            && let Some(script) = script
        {
            link_directives(plan, id, script, &mut native)?;
        }
        let mut env = environment(&plan.packages[unit.package], &unit.target)?;
        if let Some(script) = script {
            env.extend(script.environment.clone());
            env.insert(
                "OUT_DIR".into(),
                script.out_dir.to_string_lossy().into_owned(),
            );
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
        let file = if macros.is_some() {
            prepared.outputs[id]
                .artifact
                .clone()
                .ok_or("missing host macro")?
        } else {
            unit.target.root.clone()
        };
        indices.insert(id, sources.len());
        sources.push(SourceInput {
            compilation_dir: Some(plan.workspace_root.clone()),
            package_identity: None,
            host_macros: macros,
            file,
            name: unit.target.name.replace('-', "_"),
            edition: crate::cargo::edition(&unit.target.edition)?,
            dependencies,
            cfg,
            environment: Some(env),
        });
    }
    let root = *indices.get(&plan.root_unit).ok_or("missing root unit")?;
    // Link arguments apply to the package's own binaries only.
    let binary = &plan.units[plan.root_unit];
    if let Some(script) = (binary.build_script).and_then(|i| prepared.outputs[i].script.as_ref()) {
        let arguments: Vec<String> = (script.link_args.iter())
            .filter(|(name, _)| name.as_ref().is_none_or(|n| *n == binary.target.name))
            .map(|(_, argument)| argument.clone())
            .collect();
        crate::cargo::link_arguments(
            &mut native,
            &arguments,
            package_directory(plan, plan.root_unit)?,
        )?;
    }
    let output = plan
        .workspace_root
        .join("target/flux")
        .join(&plan.units[plan.root_unit].target.name);
    let success = crate::build::compile(CompileRequest {
        file: &sources[root].file,
        crate_name: &sources[root].name,
        output: &output,
        sources: &sources,
        options,
        native,
    })?;
    Ok(if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
