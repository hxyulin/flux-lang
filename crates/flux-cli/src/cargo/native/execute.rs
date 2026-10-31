//! Flux build-script compilation and native host preparation.
use super::{Domain, Options, Plan, PlannedPackage, Result, Target};
use crate::build::{CompileRequest, SourceInput};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

#[derive(Default, Serialize)]
struct ScriptOutput {
    cfgs: Vec<String>,
    check_cfgs: Vec<String>,
    environment: BTreeMap<String, String>,
    metadata: BTreeMap<String, String>,
    linker: Vec<(String, String)>,
    out_dir: PathBuf,
}
#[derive(Default, Serialize)]
struct Output {
    artifact: Option<PathBuf>,
    script: Option<ScriptOutput>,
}
#[derive(Serialize)]
struct Prepared {
    version: u32,
    kind: &'static str,
    rustc: String,
    plan: Plan,
    outputs: Vec<Output>,
}

pub(super) fn run(plan: Plan, options: &Options, optimize: Option<bool>) -> Result<ExitCode> {
    let mut rustc_units = BTreeSet::new();
    for (id, unit) in plan.units.iter().enumerate() {
        if unit.domain == Domain::Host && unit.target.kind == "proc-macro" {
            rustc_dependencies(&plan, id, &mut rustc_units);
        }
    }
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let version = if !rustc_units.is_empty() {
        crate::cargo::output(Command::new(&rustc).arg("-vV"), "rustc version")?
    } else {
        String::new()
    };
    let cfg = crate::target::cfg()?
        .into_iter()
        .collect::<Vec<_>>()
        .join("\n");
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = plan.workspace_root.join(format!(
        "target/flux/native-host/{run_id}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    let mut outputs: Vec<Output> = Vec::new();
    for (index, unit) in plan.units.iter().enumerate() {
        let mut output = Output::default();
        let script = unit.target.kind == "build-script";
        if unit.domain == Domain::Host && (script || rustc_units.contains(&index)) {
            let package = &plan.packages[unit.package];
            let mut env = environment(package, &unit.target)?;
            let built = unit.build_script.and_then(|id| outputs[id].script.as_ref());
            if let Some(built) = built {
                env.extend(built.environment.clone());
                env.insert(
                    "OUT_DIR".into(),
                    built.out_dir.to_string_lossy().into_owned(),
                );
            }
            let stem = unit.target.name.replace('-', "_");
            let artifact = if script {
                directory.join(format!("build-{index}"))
            } else if unit.target.kind == "proc-macro" {
                directory.join(format!(
                    "lib{stem}-flux{index}.{}",
                    std::env::consts::DLL_EXTENSION
                ))
            } else {
                directory.join(format!("lib{stem}-flux{index}.rlib"))
            };
            if script {
                let sources = host_sources(&plan, &outputs, index, &version)?;
                eprintln!("flux host {} build-script ({index})", package.name);
                if !crate::build::compile(CompileRequest {
                    file: &unit.target.root,
                    crate_name: &stem,
                    output: &artifact,
                    sources: &sources,
                    options: crate::BuildOptions {
                        optimize: false,
                        timings: false,
                    },
                })? {
                    return Err(format!(
                        "cannot compile build script for {} with flux",
                        package.name
                    ));
                }
            } else {
                let mut compile = Command::new(&rustc);
                clean_environment(&mut compile);
                compile
                    .current_dir(package.manifest.parent().ok_or("manifest has no parent")?)
                    .envs(&env)
                    .arg(&unit.target.root)
                    .arg(format!("--edition={}", unit.target.edition))
                    .args([
                        "--crate-name",
                        &stem,
                        "--crate-type",
                        if script {
                            "bin"
                        } else if unit.target.kind == "proc-macro" {
                            "proc-macro"
                        } else {
                            "rlib"
                        },
                    ])
                    .arg(format!("-Cmetadata=flux_native_{index}"))
                    .arg("-L")
                    .arg(format!("dependency={}", directory.display()))
                    .arg("-o")
                    .arg(&artifact);
                if package.source.is_some() {
                    compile.args(["--cap-lints", "allow"]);
                }
                if unit.target.kind == "proc-macro" {
                    compile.args(["--extern", "proc_macro"]);
                }
                for feature in &unit.features {
                    compile.arg("--cfg").arg(format!("feature={feature:?}"));
                }
                if let Some(built) = built {
                    for cfg in &built.cfgs {
                        compile.arg("--cfg").arg(cfg);
                    }
                    for cfg in &built.check_cfgs {
                        compile.arg("--check-cfg").arg(cfg);
                    }
                    for (flag, value) in &built.linker {
                        compile.arg(flag).arg(value);
                    }
                }
                for (alias, dependency) in &unit.dependencies {
                    let dependency = outputs[*dependency]
                        .artifact
                        .as_ref()
                        .ok_or("missing compiled host dependency")?;
                    compile
                        .arg("--extern")
                        .arg(format!("{alias}={}", dependency.display()));
                }
                eprintln!("rustc host {} {} ({index})", package.name, unit.target.kind);
                crate::cargo::output(&mut compile, "native host compilation")?;
            }
            if script {
                let unit_directory = directory.join(format!("unit-{index}"));
                let out = unit_directory.join("out");
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
                    .env("RUSTC", &rustc)
                    .env("CARGO_ENCODED_RUSTFLAGS", "");
                if let Some(links) = &package.links {
                    run.env("CARGO_MANIFEST_LINKS", links);
                }
                for feature in &unit.features {
                    run.env(format!("CARGO_FEATURE_{}", env_name(feature)?), "1");
                }
                for (name, value) in cfg_environment(&cfg)? {
                    run.env(name, value);
                }
                for (name, value) in dependency_metadata(&plan, index, &outputs)? {
                    run.env(name, value);
                }
                let result = run
                    .output()
                    .map_err(|e| format!("cannot run build script for {}: {e}", package.name))?;
                std::fs::write(unit_directory.join("stdout"), &result.stdout)
                    .map_err(|e| e.to_string())?;
                std::fs::write(unit_directory.join("stderr"), &result.stderr)
                    .map_err(|e| e.to_string())?;
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
            }
            output.artifact = Some(artifact);
        }
        outputs.push(output);
    }
    let prepared = Prepared {
        version: 1,
        kind: "flux-native-prepared",
        rustc: version,
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
    if let Some(optimize) = optimize {
        build(&prepared, optimize)
    } else {
        Ok(ExitCode::SUCCESS)
    }
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

fn host_sources(
    plan: &Plan,
    outputs: &[Output],
    root: usize,
    rustc: &str,
) -> Result<Vec<SourceInput>> {
    fn visit(
        plan: &Plan,
        outputs: &[Output],
        id: usize,
        rustc: &str,
        indices: &mut BTreeMap<usize, usize>,
        sources: &mut Vec<SourceInput>,
    ) -> Result<usize> {
        if let Some(index) = indices.get(&id) {
            return Ok(*index);
        }
        let unit = &plan.units[id];
        let script = unit.build_script.and_then(|i| outputs[i].script.as_ref());
        let mut cfg = crate::cargo::cfg(script.map_or(&[], |s| s.cfgs.as_slice()), &unit.features)?;
        let mut environment = environment(&plan.packages[unit.package], &unit.target)?;
        if let Some(script) = script {
            if !script.linker.is_empty() {
                return Err("native host linker directives require Flux linker integration".into());
            }
            environment.extend(script.environment.clone());
            environment.insert(
                "OUT_DIR".into(),
                script.out_dir.to_string_lossy().into_owned(),
            );
        }
        let macros = if unit.target.kind == "proc-macro" {
            cfg.names.push(flux_intern::Symbol::intern("proc_macro"));
            Some(crate::proc_macro::exports(
                rustc,
                outputs[id].artifact.as_ref().ok_or("missing host macro")?,
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
                    visit(plan, outputs, *dependency, rustc, indices, sources)?,
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
    visit(
        plan,
        outputs,
        root,
        rustc,
        &mut BTreeMap::new(),
        &mut sources,
    )?;
    Ok(sources)
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

fn build(prepared: &Prepared, optimize: bool) -> Result<ExitCode> {
    let plan = &prepared.plan;
    let mut sources = Vec::new();
    let mut indices = BTreeMap::new();
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
            Some(crate::proc_macro::exports(
                &prepared.rustc,
                prepared.outputs[id]
                    .artifact
                    .as_ref()
                    .ok_or("missing host macro")?,
                &unit.target.root,
                cfg.clone(),
            )?)
        } else {
            None
        };
        if macros.is_none() && script.is_some_and(|s| !s.linker.is_empty()) {
            return Err(
                "native build-script linker directives require Flux linker integration".into(),
            );
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
    let output = plan
        .workspace_root
        .join("target/flux")
        .join(&plan.units[plan.root_unit].target.name);
    let success = crate::build::compile(CompileRequest {
        file: &sources[root].file,
        crate_name: &sources[root].name,
        output: &output,
        sources: &sources,
        options: crate::BuildOptions {
            optimize,
            timings: false,
        },
    })?;
    Ok(if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
