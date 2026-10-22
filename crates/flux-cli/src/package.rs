//! A deliberately bounded Cargo manifest driver, not a rustc adapter.
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
    let plan = plan(options)?;
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err("package builds support only arm64 macOS hosts".into());
    }
    std::fs::create_dir_all(plan.output.parent().unwrap())
        .map_err(|e| format!("cannot create output directory: {e}"))?;
    if !build::compile(CompileRequest {
        file: &plan.source,
        crate_name: &plan.crate_name,
        output: &plan.output,
        sources: &plan.sources,
        options: BuildOptions {
            optimize: options.optimize,
            timings: false,
        },
    })? {
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

fn plan(options: &Options) -> Result<Plan, String> {
    let manifest = canonical_manifest(&options.manifest_path)?;
    let root = manifest.parent().unwrap();
    let mut loader = SourceGraph {
        manifests: Vec::new(),
        sources: Vec::new(),
        active: Vec::new(),
        target_names: Vec::new(),
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

struct SourceGraph {
    manifests: Vec<PathBuf>,
    sources: Vec<SourceInput>,
    active: Vec<PathBuf>,
    target_names: Vec<String>,
}

impl SourceGraph {
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
        let package = metadata_package(manifest)?;
        let target = select_target(&package, library, bin)?;
        let index = self.sources.len();
        self.sources.push(SourceInput {
            file: target.src_path.clone(),
            name: target.name.replace('-', "_"),
            dependencies,
        });
        self.target_names.push(target.name.clone());
        self.manifests.push(manifest.to_owned());
        self.active.pop();
        Ok(index)
    }
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
    if package.edition != "2024" {
        return Err("package builds require edition 2024".into());
    }
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
    for target in &package.targets {
        if target.kind != [kind] || target.crate_types != [kind] {
            return Err(format!(
                "target `{}` in `{}` is not a supported {} target",
                target.name,
                package.name,
                if library { "library" } else { "binary" }
            ));
        }
        if target.edition != "2024" {
            return Err("targets require edition 2024".into());
        }
    }
    let candidates: Vec<_> = package
        .targets
        .iter()
        .filter(|t| bin.is_none_or(|name| name == t.name))
        .collect();
    match candidates.as_slice() {
        [target] => Ok(*target),
        [] => Err(format!(
            "expected a matching {} target",
            if library { "library" } else { "binary" }
        )),
        _ => Err("multiple binary targets; select one with --bin".into()),
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
            &["package", "lib", "dependencies"]
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
        ],
        "package option",
    )?;
    if package.get("edition").and_then(toml::Value::as_str) != Some("2024") {
        return Err("package builds require edition 2024".into());
    }
    if path.parent().unwrap().join("build.rs").exists() {
        return Err("build.rs is not supported yet".into());
    }
    if let Some(lib) = table.get("lib") {
        allowed_keys(
            lib.as_table().ok_or("expected a library table")?,
            &["name", "path"],
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
