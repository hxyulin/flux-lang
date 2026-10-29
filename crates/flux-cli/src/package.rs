//! A bounded manifest driver. Target crates use Flux; macro hosts bootstrap with rustc.
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

use crate::BuildOptions;
use crate::build::{self, CompileRequest, SourceInput};

#[derive(clap::Subcommand)]
pub(crate) enum Command {
    /// Always rebuild one binary with Flux/Keel (offline metadata only).
    Build(Options),
    /// Rebuild and run one binary, preserving its exit status.
    Run {
        #[command(flatten)]
        options: Options,
        #[arg(last = true)]
        args: Vec<String>,
    },
}

#[derive(clap::Args)]
pub(crate) struct Options {
    #[arg(long)]
    manifest_path: PathBuf,
    #[arg(long)]
    bin: Option<String>,
    /// Enable Flux/Keel backend optimization (not a Cargo release profile).
    #[arg(short = 'O', long)]
    optimize: bool,
    /// Print scoped compilation timings (excludes execution of the program).
    #[arg(long)]
    timings: bool,
    /// Select the compiler used to build procedural macro host executables.
    #[arg(long, value_enum, default_value_t = MacroHostCompiler::Rustc)]
    macro_host_compiler: MacroHostCompiler,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum MacroHostCompiler {
    Rustc,
    Flux,
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    workspace_root: PathBuf,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    manifest_path: PathBuf,
    edition: String,
    targets: Vec<Target>,
    features: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
    crate_types: Vec<String>,
    src_path: PathBuf,
    edition: String,
}

struct Plan {
    source: PathBuf,
    crate_name: String,
    output: PathBuf,
    sources: Vec<SourceInput>,
}

pub(crate) fn execute(command: &Command) -> Result<ExitCode, String> {
    let (Command::Build(options) | Command::Run { options, .. }) = command;
    // This boundary includes planning, output-directory setup and compilation
    // (including its timing report), but stops before execution. On success the
    // returned Plan is destroyed outside this scope.
    // CLI parsing, final driver error formatting and process teardown are outside.
    let (plan, compiled) = {
        let mut timings = build::Timings::new(options.timings, "package compile scoped elapsed");
        let plan = plan(options, &mut timings)?;
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err("package builds support only arm64 macOS hosts".into());
        }
        std::fs::create_dir_all(plan.output.parent().unwrap())
            .map_err(|e| format!("cannot create output directory: {e}"))?;
        let compiled = build::compile(CompileRequest {
            file: &plan.source,
            crate_name: &plan.crate_name,
            output: &plan.output,
            sources: &plan.sources,
            options: BuildOptions {
                optimize: options.optimize,
                timings: options.timings,
            },
        })?;
        (plan, compiled)
    };
    if !compiled {
        return Ok(ExitCode::FAILURE);
    }
    if let Command::Run { args, .. } = command {
        let status = std::process::Command::new(&plan.output)
            .args(args)
            .status()
            .map_err(|e| format!("cannot run {}: {e}", plan.output.display()))?;
        Ok(ExitCode::from(build::exit_code(status)))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn plan(options: &Options, timings: &mut build::Timings) -> Result<Plan, String> {
    // Keep the inclusive planning duration even when validation or metadata fails.
    let start = options.timings.then(std::time::Instant::now);
    let result = plan_inner(options, timings);
    if let Some(start) = start {
        timings.record("planning (inclusive of metadata)", start.elapsed());
    }
    result
}

fn plan_inner(options: &Options, timings: &mut build::Timings) -> Result<Plan, String> {
    let manifest = canonical_manifest(&options.manifest_path)?;
    let root = manifest.parent().unwrap();
    let mut loader = SourceGraph {
        timings,
        manifests: Vec::new(),
        sources: Vec::new(),
        active: Vec::new(),
        target_names: Vec::new(),
        host_artifacts: Vec::new(),
        next_host_id: 0,
        active_hosts: Vec::new(),
        host_output: root.join("target/flux-host"),
        macro_host_compiler: options.macro_host_compiler,
    };
    let index = loader.visit(&manifest, false, options.bin.as_deref())?;
    let target_name = &loader.target_names[index];
    let main = loader.sources.last().unwrap();
    // Cargo validates target names. Keep the output inside our namespace too.
    let mut components = Path::new(target_name).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("unsupported binary target name".into());
    }
    // The output name retains Cargo's target spelling, unlike the Rust crate name.
    Ok(Plan {
        source: main.file.clone(),
        crate_name: main.name.clone(),
        output: root.join("target/flux").join(target_name),
        sources: loader.sources,
    })
}

fn canonical_manifest(path: &Path) -> Result<PathBuf, String> {
    let manifest = path
        .canonicalize()
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if manifest.file_name().is_none_or(|name| name != "Cargo.toml") {
        return Err("expected a Cargo.toml manifest".into());
    }
    Ok(manifest)
}

struct SourceGraph<'a> {
    macro_host_compiler: MacroHostCompiler,
    timings: &'a mut build::Timings,
    manifests: Vec<PathBuf>,
    sources: Vec<SourceInput>,
    active: Vec<PathBuf>,
    target_names: Vec<String>,
    host_artifacts: Vec<(PathBuf, PathBuf)>,
    next_host_id: usize,
    active_hosts: Vec<PathBuf>,
    host_output: PathBuf,
}

impl SourceGraph<'_> {
    fn visit(
        &mut self,
        manifest: &Path,
        library: bool,
        bin: Option<&str>,
    ) -> Result<usize, String> {
        if self.active.iter().any(|p| p == manifest) {
            return Err(format!(
                "cyclic path dependencies at {}",
                manifest.display()
            ));
        }
        if let Some(index) = self.manifests.iter().position(|p| p == manifest) {
            return Ok(index);
        }
        self.active.push(manifest.to_owned());
        validate_manifest(manifest, library)?;
        let root = manifest.parent().unwrap();
        validate_configuration(root)?;
        let table = read_toml(manifest)?;
        if library && is_proc_macro(&table) {
            return self.visit_proc_macro(manifest, &table);
        }
        let mut dependencies = Vec::new();
        if let Some(deps) = table.get("dependencies") {
            let deps = deps.as_table().ok_or("expected a dependencies table")?;
            for (alias, value) in deps {
                let spec = value
                    .as_table()
                    .ok_or("only local path dependencies are supported yet")?;
                allowed_keys(spec, &["path", "package"], "dependency option")?;
                let path = spec
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .ok_or("only local path dependencies are supported yet")?;
                let child = canonical_manifest(&root.join(path).join("Cargo.toml"))?;
                let expected = spec
                    .get("package")
                    .map(|p| p.as_str().ok_or("expected a dependency package name"))
                    .transpose()?
                    .unwrap_or(alias);
                let child_table = read_toml(&child)?;
                if child_table
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(toml::Value::as_str)
                    != Some(expected)
                {
                    return Err(format!(
                        "path dependency `{alias}` does not name package `{expected}`"
                    ));
                }
                let index = self.visit(&child, true, None)?;
                let name = if spec.contains_key("package") {
                    alias.replace('-', "_")
                } else {
                    self.sources[index].name.clone()
                };
                if matches!(
                    name.as_str(),
                    "core" | "alloc" | "std" | "self" | "super" | "crate"
                ) {
                    return Err(format!("dependency name `{name}` is reserved"));
                }
                if dependencies.iter().any(|(n, _)| n == &name) {
                    return Err(format!("duplicate dependency import name `{name}`"));
                }
                dependencies.push((name, index));
            }
        }
        let package = self
            .timings
            .time("  cargo metadata + decode/validate", || {
                metadata_package(manifest)
            })?;
        let target = select_target(&package, library, bin)?;
        let index = self.sources.len();
        self.sources.push(SourceInput {
            edition: edition(&target.edition)?,
            file: target.src_path.clone(),
            name: target.name.replace('-', "_"),
            dependencies,
            host_macros: None,
            compilation_dir: None,
            cfg: Default::default(),
            environment: None,
        });
        self.target_names.push(target.name.clone());
        self.manifests.push(manifest.to_owned());
        self.active.pop();
        Ok(index)
    }

    fn host_directory(&mut self) -> Result<PathBuf, String> {
        let directory = self.host_output.join(format!("host-{}", self.next_host_id));
        self.next_host_id += 1;
        std::fs::create_dir_all(&directory)
            .map_err(|e| format!("cannot create macro host directory: {e}"))?;
        Ok(directory)
    }

    fn visit_proc_macro(&mut self, manifest: &Path, table: &toml::Table) -> Result<usize, String> {
        let package = self
            .timings
            .time("  cargo metadata + decode/validate", || {
                metadata_package(manifest)
            })?;
        let target = host_target(&package, true)?;
        let directory = self.host_directory()?;
        let proc_macros = if matches!(self.macro_host_compiler, MacroHostCompiler::Flux) {
            if table
                .get("dependencies")
                .and_then(toml::Value::as_table)
                .is_some_and(|deps| !deps.is_empty())
            {
                return Err(
                    "dependencies of flux-compiled macro hosts are not supported yet".into(),
                );
            }
            crate::proc_macro_native::build_host(
                &target.src_path,
                &target.name.replace('-', "_"),
                edition(&target.edition)?,
                &directory.join("host"),
            )?
        } else {
            let dependencies = self.host_dependencies(manifest, table)?;
            crate::proc_macro::local::build_host(
                &target.src_path,
                &target.name.replace('-', "_"),
                edition(&target.edition)?,
                &directory.join("host"),
                &dependencies,
                &self
                    .host_artifacts
                    .iter()
                    .filter_map(|(_, library)| library.parent().map(Path::to_path_buf))
                    .collect::<Vec<_>>(),
                manifest.parent().unwrap(),
            )?
        };
        // The target graph sees only macro exports, never the host implementation.
        let empty = directory.join("exports.rs");
        std::fs::write(&empty, "")
            .map_err(|e| format!("cannot write macro exports source: {e}"))?;
        let index = self.sources.len();
        self.sources.push(SourceInput {
            edition: edition(&target.edition)?,
            file: empty,
            name: target.name.replace('-', "_"),
            dependencies: Vec::new(),
            host_macros: Some(proc_macros),
            compilation_dir: None,
            cfg: Default::default(),
            environment: None,
        });
        self.target_names.push(target.name.clone());
        self.manifests.push(manifest.to_owned());
        self.active.pop();
        Ok(index)
    }

    fn host_dependencies(
        &mut self,
        manifest: &Path,
        table: &toml::Table,
    ) -> Result<Vec<(String, PathBuf)>, String> {
        let mut dependencies = Vec::new();
        if let Some(deps) = table.get("dependencies") {
            for (alias, value) in deps.as_table().ok_or("expected a dependencies table")? {
                let spec = value
                    .as_table()
                    .ok_or("only local path dependencies are supported yet")?;
                allowed_keys(spec, &["path", "package"], "dependency option")?;
                let path = spec
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .ok_or("only local path dependencies are supported yet")?;
                let child =
                    canonical_manifest(&manifest.parent().unwrap().join(path).join("Cargo.toml"))?;
                let child_table = read_toml(&child)?;
                let expected = spec
                    .get("package")
                    .map(|p| p.as_str().ok_or("expected a dependency package name"))
                    .transpose()?
                    .unwrap_or(alias);
                if child_table
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(toml::Value::as_str)
                    != Some(expected)
                {
                    return Err(format!(
                        "path dependency `{alias}` does not name package `{expected}`"
                    ));
                }
                let name = if spec.contains_key("package") {
                    alias.replace('-', "_")
                } else {
                    child_table
                        .get("lib")
                        .and_then(|lib| lib.get("name"))
                        .and_then(toml::Value::as_str)
                        .unwrap_or(alias)
                        .replace('-', "_")
                };
                if matches!(
                    name.as_str(),
                    "core" | "alloc" | "std" | "self" | "super" | "crate" | "proc_macro"
                ) {
                    return Err(format!("host dependency name `{name}` is reserved"));
                }
                if dependencies.iter().any(|(other, _)| other == &name) {
                    return Err(format!("duplicate host dependency import name `{name}`"));
                }
                dependencies.push((name, self.host_artifact(&child)?));
            }
        }
        Ok(dependencies)
    }

    fn host_artifact(&mut self, manifest: &Path) -> Result<PathBuf, String> {
        if self.active_hosts.iter().any(|p| p == manifest)
            || self.active.iter().any(|p| p == manifest)
        {
            return Err(format!(
                "cyclic path dependencies at {}",
                manifest.display()
            ));
        }
        if let Some((_, output)) = self.host_artifacts.iter().find(|(p, _)| p == manifest) {
            return Ok(output.clone());
        }
        self.active_hosts.push(manifest.to_owned());
        validate_manifest(manifest, true)?;
        validate_configuration(manifest.parent().unwrap())?;
        let table = read_toml(manifest)?;
        let package = self
            .timings
            .time("  cargo metadata + decode/validate", || {
                metadata_package(manifest)
            })?;
        let proc_macro = is_proc_macro(&table);
        let target = host_target(&package, proc_macro)?;
        let dependencies = self.host_dependencies(manifest, &table)?;
        let directory = self.host_directory()?;
        let name = target.name.replace('-', "_");
        let extension = if proc_macro {
            std::env::consts::DLL_EXTENSION
        } else {
            "rlib"
        };
        let output = directory.join(format!("lib{name}.{extension}"));
        let crate_type = if proc_macro { "proc-macro" } else { "rlib" };
        let mut command = std::process::Command::new("rustc");
        command
            .current_dir(manifest.parent().unwrap())
            .env("CARGO_MANIFEST_DIR", manifest.parent().unwrap())
            .arg(&target.src_path)
            .args([
                "--crate-type",
                crate_type,
                "--crate-name",
                &name,
                "--edition",
                &target.edition,
            ])
            .args(["-C", &format!("metadata=flux_host_{}", self.next_host_id)])
            .arg("-o")
            .arg(&output);
        for (alias, library) in &dependencies {
            command
                .arg("--extern")
                .arg(format!("{alias}={}", library.display()));
        }
        if proc_macro {
            // Cargo normally exposes the compiler's proc_macro crate this way.
            command.args(["--extern", "proc_macro"]);
        }
        // rustc also loads transitive metadata, including helpers shared by hosts.
        for (_, library) in &self.host_artifacts {
            if let Some(parent) = library.parent() {
                command
                    .arg("-L")
                    .arg(format!("dependency={}", parent.display()));
            }
        }
        let result = command
            .output()
            .map_err(|e| format!("cannot compile macro host dependency: {e}"))?;
        if !result.status.success() {
            return Err(format!(
                "macro host dependency `{name}` failed: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
        self.active_hosts.pop();
        self.host_artifacts
            .push((manifest.to_owned(), output.clone()));
        Ok(output)
    }
}

fn is_proc_macro(table: &toml::Table) -> bool {
    table
        .get("lib")
        .and_then(|lib| lib.get("proc-macro"))
        .and_then(toml::Value::as_bool)
        == Some(true)
}

fn host_target(package: &Package, proc_macro: bool) -> Result<&Target, String> {
    if package
        .targets
        .iter()
        .any(|target| target.kind.iter().any(|kind| kind == "custom-build"))
    {
        return Err("build scripts are not supported yet".into());
    }
    let kind = if proc_macro { "proc-macro" } else { "lib" };
    let target = package
        .targets
        .iter()
        .find(|target| target.kind == [kind])
        .ok_or_else(|| format!("expected a matching {kind} target"))?;
    if target.crate_types != [kind] {
        return Err("unsupported host target crate types".into());
    }
    edition(&target.edition)?;
    Ok(target)
}

fn metadata_package(manifest: &Path) -> Result<Package, String> {
    let root = manifest.parent().unwrap();
    let output = std::process::Command::new("cargo")
        .current_dir(root)
        .args([
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("invalid cargo metadata: {e}"))?;
    if metadata.packages.len() != 1
        || metadata.workspace_members.len() != 1
        || metadata.workspace_root.canonicalize().ok().as_deref() != Some(root)
    {
        return Err("workspaces are not supported yet; expected one standalone package".into());
    }
    let package = metadata.packages.into_iter().next().unwrap();
    if package.manifest_path.canonicalize().ok().as_deref() != Some(manifest)
        || metadata.workspace_members[0] != package.id
    {
        return Err("expected the requested standalone package".into());
    }
    edition(&package.edition)?;
    if !package.features.is_empty() {
        return Err("features are not supported yet".into());
    }
    Ok(package)
}

fn select_target<'a>(
    package: &'a Package,
    library: bool,
    bin: Option<&str>,
) -> Result<&'a Target, String> {
    let kind = if library { "lib" } else { "bin" };
    if !library
        && package
            .targets
            .iter()
            .any(|t| t.kind != ["bin"] || t.crate_types != ["bin"])
    {
        return Err(format!(
            "target in `{}` is not a supported binary target",
            package.name
        ));
    }
    // Only the requested unit is compiled. Tests, benches, examples and other
    // binaries do not contribute dependencies, cfgs or code to a library unit.
    if package.targets.iter().any(|t| {
        t.kind
            .iter()
            .any(|k| k == "custom-build" || k == "proc-macro")
    }) {
        return Err("build scripts and proc-macro targets are not supported yet".into());
    }
    let candidates: Vec<_> = package
        .targets
        .iter()
        .filter(|t| t.kind == [kind] && bin.is_none_or(|name| name == t.name))
        .collect();
    for target in &candidates {
        if target.crate_types != [kind] {
            return Err("unsupported target crate types".into());
        }
        edition(&target.edition)?;
    }
    match candidates.as_slice() {
        [target] => Ok(*target),
        [] => Err(format!(
            "expected a matching {} target",
            if library { "library" } else { "binary" }
        )),
        _ => Err("multiple binary targets; select one with --bin".into()),
    }
}

fn edition(value: &str) -> Result<flux_syntax::Edition, String> {
    match value {
        "2018" => Ok(flux_syntax::Edition::E2018),
        "2021" => Ok(flux_syntax::Edition::E2021),
        "2024" => Ok(flux_syntax::Edition::E2024),
        _ => Err("package builds require edition 2018, 2021 or 2024".into()),
    }
}

fn read_toml(path: &Path) -> Result<toml::Table, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("invalid TOML in {}: {e}", path.display()))
}

fn allowed_keys(table: &toml::Table, allowed: &[&str], context: &str) -> Result<(), String> {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("{context} `{key}` is not supported yet"));
        }
    }
    Ok(())
}

fn validate_manifest(path: &Path, library: bool) -> Result<(), String> {
    let table = read_toml(path)?;
    allowed_keys(
        &table,
        if library {
            &[
                "package",
                "lib",
                "bin",
                "test",
                "bench",
                "example",
                "dev-dependencies",
                "target",
                "dependencies",
                "features",
            ]
        } else {
            &["package", "bin", "dependencies"]
        },
        "manifest declaration",
    )?;
    let package = table
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or("expected a standalone [package]")?;
    allowed_keys(
        package,
        &[
            "name",
            "version",
            "edition",
            "authors",
            "description",
            "license",
            "license-file",
            "readme",
            "repository",
            "homepage",
            "documentation",
            "keywords",
            "categories",
            "publish",
            "rust-version",
            "metadata",
            "build",
            "autolib",
            "autobins",
            "autoexamples",
            "autotests",
            "autobenches",
        ],
        "package option",
    )?;
    edition(
        package
            .get("edition")
            .and_then(toml::Value::as_str)
            .unwrap_or("2015"),
    )?;
    if let Some(build) = package.get("build")
        && build.as_bool() != Some(false)
    {
        return Err("build scripts are not supported yet (only build = false is accepted)".into());
    }
    if package.get("build").and_then(toml::Value::as_bool) != Some(false)
        && path.parent().unwrap().join("build.rs").exists()
    {
        return Err("build.rs is not supported yet".into());
    }
    // Target-specific dev dependencies are inert for the selected library.
    // Never silently ignore a target-specific normal or build dependency.
    if let Some(targets) = table.get("target") {
        for value in targets
            .as_table()
            .ok_or("expected a target table")?
            .values()
        {
            allowed_keys(
                value
                    .as_table()
                    .ok_or("expected a target configuration table")?,
                &["dev-dependencies"],
                "target declaration",
            )?;
        }
    }
    if let Some(lib) = table.get("lib") {
        if let Some(value) = lib.get("proc-macro")
            && value.as_bool().is_none()
        {
            return Err("expected a boolean proc-macro option".into());
        }
        allowed_keys(
            lib.as_table().ok_or("expected a library table")?,
            &["name", "path", "test", "bench", "doctest", "proc-macro"],
            "library option",
        )?;
    }
    if let Some(bins) = table.get("bin") {
        let bins = bins.as_array().ok_or("expected [[bin]] declarations")?;
        for bin in bins {
            allowed_keys(
                bin.as_table().ok_or("expected a binary table")?,
                &["name", "path"],
                "binary option",
            )?;
        }
    }
    Ok(())
}

fn validate_configuration(root: &Path) -> Result<(), String> {
    // Cargo is run from the package root, so these are its config search paths.
    let mut directories: Vec<_> = root.ancestors().map(|dir| dir.join(".cargo")).collect();
    if let Some(home) = std::env::var_os("CARGO_HOME") {
        let home = PathBuf::from(home);
        if !home.is_absolute() {
            return Err("expected an absolute CARGO_HOME".into());
        }
        directories.push(home);
    } else if let Some(home) = std::env::var_os("HOME") {
        directories.push(PathBuf::from(home).join(".cargo"));
    }
    for directory in directories {
        for name in ["config", "config.toml"] {
            let path = directory.join(name);
            let exists = path
                .try_exists()
                .map_err(|e| format!("cannot inspect {}: {e}", path.display()))?;
            if exists && !read_toml(&path)?.is_empty() {
                return Err(format!(
                    "cargo configuration is not supported yet: {}",
                    path.display()
                ));
            }
        }
    }
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if matches!(
            name.as_ref(),
            "CARGO_INCREMENTAL"
                | "RUSTFLAGS"
                | "CARGO_ENCODED_RUSTFLAGS"
                | "RUSTDOCFLAGS"
                | "CARGO_ENCODED_RUSTDOCFLAGS"
        ) || name.starts_with("CARGO_BUILD_")
            || name.starts_with("CARGO_TARGET_")
            || name.starts_with("CARGO_PROFILE_")
            || name.starts_with("CARGO_UNSTABLE_")
        {
            return Err(format!(
                "cargo environment option `{name}` is not supported yet"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "package_edition_tests.rs"]
mod edition_tests;
