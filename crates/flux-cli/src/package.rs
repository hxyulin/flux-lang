//! A deliberately bounded Cargo manifest driver, not a rustc adapter.
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

use crate::BuildOptions;
use crate::build::{self, CompileRequest};

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
    manifest_path: PathBuf,
    edition: String,
    targets: Vec<Target>,
    dependencies: Vec<serde_json::Value>,
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
        options: BuildOptions {
            optimize: false,
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
    let manifest = options
        .manifest_path
        .canonicalize()
        .map_err(|e| format!("cannot read {}: {e}", options.manifest_path.display()))?;
    if manifest.file_name().is_none_or(|name| name != "Cargo.toml") {
        return Err("expected a Cargo.toml manifest".into());
    }
    let root = manifest.parent().unwrap();
    validate_manifest(&manifest)?;
    validate_configuration(root)?;
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
        .arg(&manifest)
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
    let package = &metadata.packages[0];
    if package.manifest_path.canonicalize().ok().as_deref() != Some(manifest.as_path())
        || metadata.workspace_members[0] != package.id
    {
        return Err("expected the requested standalone package".into());
    }
    if package.edition != "2024" {
        return Err("package builds require edition 2024".into());
    }
    if !package.dependencies.is_empty() || !package.features.is_empty() {
        return Err("dependencies and features are not supported yet".into());
    }
    for target in &package.targets {
        if target.kind != ["bin"] || target.crate_types != ["bin"] {
            return Err(format!(
                "target `{}` is not a supported binary target",
                target.name
            ));
        }
        if target.edition != "2024" {
            return Err("binary targets require edition 2024".into());
        }
    }
    let candidates: Vec<_> = package
        .targets
        .iter()
        .filter(|target| options.bin.as_ref().is_none_or(|name| *name == target.name))
        .collect();
    let target = match candidates.as_slice() {
        [target] => *target,
        [] => return Err("expected a matching binary target".into()),
        _ => return Err("multiple binary targets; select one with --bin".into()),
    };
    // Cargo validates target names. Still keep the output inside our namespace.
    let mut components = Path::new(&target.name).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("unsupported binary target name".into());
    }
    Ok(Plan {
        source: target.src_path.clone(),
        crate_name: target.name.replace('-', "_"),
        output: root.join("target/flux").join(&target.name),
    })
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

fn validate_manifest(path: &Path) -> Result<(), String> {
    let table = read_toml(path)?;
    allowed_keys(&table, &["package", "bin"], "manifest declaration")?;
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
