//! Lockfile-only source acquisition. No dependency resolution or host compilation.
use super::{Lock, Locked, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod archive;
const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";
const CACHE: &str = "index.crates.io-1949cf8c6b5b557f";
const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;

#[derive(clap::Args)]
pub(crate) struct FetchOptions {
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,
    /// Require an existing lockfile; version resolution is not supported yet.
    #[arg(long)]
    locked: bool,
    /// Use cached sources or archives only; never download missing sources.
    #[arg(long)]
    offline: bool,
    /// Override the crates.io download API base, retaining lockfile checksums.
    #[arg(long, default_value = "https://crates.io/api/v1/crates")]
    download_base: String,
}

pub(super) fn run(options: &FetchOptions) -> Result<ExitCode> {
    if !options.locked {
        return Err("dependency resolution is not supported yet; use `fetch --locked` with an existing Cargo.lock".into());
    }
    let manifest = super::canonical(&options.manifest_path)?;
    let (root, table) = super::workspace(&manifest)?;
    super::validate_configuration(&root, &table, false)?;
    let lock_path = root.join("Cargo.lock");
    let original = std::fs::read(&lock_path)
        .map_err(|e| format!("cannot read {}: {e}", lock_path.display()))?;
    let text = std::str::from_utf8(&original).map_err(|e| e.to_string())?;
    let lock: Lock = toml::from_str(text).map_err(|e| format!("invalid Cargo.lock: {e}"))?;
    if !matches!(lock.version, 3 | 4) {
        return Err("fetching requires Cargo.lock version 3 or 4".into());
    }
    // Validate every identity before creating cache entries or contacting a server.
    for package in &lock.package {
        validate(package)?;
    }
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")))
        .ok_or("set CARGO_HOME to locate dependency sources")?;
    let mut repositories = BTreeSet::new();
    let mut count = 0;
    for package in &lock.package {
        let Some(source) = &package.source else {
            continue;
        };
        if let Some(path) = super::cached_source(&home, package)? {
            validate_manifest(&path, package)?;
            count += 1;
            continue;
        }
        if source == REGISTRY {
            registry(&home, package, options)?;
        } else {
            if repositories.insert(source.clone()) {
                git(&home, source, options.offline)?;
            }
            let path = super::cached_source(&home, package)?.ok_or_else(|| {
                format!(
                    "locked git checkout does not contain {} {}",
                    package.name, package.version
                )
            })?;
            validate_manifest(&path, package)?;
        }
        count += 1;
    }
    if std::fs::read(&lock_path).map_err(|e| e.to_string())? != original {
        return Err("Cargo.lock changed while fetching; retry with a stable lockfile".into());
    }
    eprintln!("fetched {count} locked source packages without Cargo");
    Ok(ExitCode::SUCCESS)
}
fn validate(package: &Locked) -> Result<()> {
    if package.name.is_empty()
        || !package
            .name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err("invalid locked package name".into());
    }
    semver::Version::parse(&package.version).map_err(|e| format!("invalid locked version: {e}"))?;
    if let Some(source) = &package.source {
        if source == REGISTRY {
            let checksum = package
                .checksum
                .as_deref()
                .ok_or("registry lock entry is missing its checksum")?;
            if checksum.len() != 64
                || !checksum
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err("expected a lowercase SHA-256 registry checksum".into());
            }
        } else {
            git_identity(source)?;
        }
    }
    Ok(())
}
fn registry(home: &Path, package: &Locked, options: &FetchOptions) -> Result<()> {
    let name = format!("{}-{}", package.name, package.version);
    let archive_dir = home.join("registry/cache").join(CACHE);
    let source_dir = home.join("registry/src").join(CACHE);
    let archive_path = archive_dir.join(format!("{name}.crate"));
    if !archive_path.is_file() {
        if options.offline {
            return Err(format!("offline source missing for {name}"));
        }
        let temporary = Temporary::new(&archive_dir)?;
        let path = temporary.0.join("download.crate");
        let url = format!(
            "{}/{}/{}/download",
            options.download_base.trim_end_matches('/'),
            package.name,
            package.version
        );
        download(&url, &path)?;
        verify_archive(&path, package)?;
        std::fs::rename(&path, &archive_path)
            .map_err(|e| format!("cannot install cached archive: {e}"))?;
    }
    verify_archive(&archive_path, package)?;
    let temporary = Temporary::new(&source_dir)?;
    let files = archive::unpack(&archive_path, &temporary.0, &name)?;
    let extracted = temporary.0.join(&name);
    validate_manifest(&extracted.join("Cargo.toml"), package)?;
    let metadata = serde_json::json!({"package":package.checksum,"files":files});
    std::fs::write(
        extracted.join(".cargo-checksum.json"),
        serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(extracted.join(".cargo-ok"), b"{\"v\":1}").map_err(|e| e.to_string())?;
    let destination = source_dir.join(&name);
    if let Err(error) = std::fs::rename(&extracted, &destination) {
        // Another fetch may have installed this same immutable package first.
        if !destination.is_dir() {
            return Err(format!("cannot install {name}: {error}"));
        }
        validate_manifest(&destination.join("Cargo.toml"), package)?;
        let metadata: serde_json::Value =
            serde_json::from_str(&super::read(&destination.join(".cargo-checksum.json"))?)
                .map_err(|e| e.to_string())?;
        if metadata["package"].as_str() != package.checksum.as_deref() {
            return Err(format!("conflicting cached package {name}"));
        }
    }
    eprintln!("fetched registry {name}");
    Ok(())
}
fn download(url: &str, destination: &Path) -> Result<()> {
    let protocols = if url.starts_with("https://") {
        "=https"
    } else if url.starts_with("http://") {
        "=http,https"
    } else if url.starts_with("file://") {
        "=file"
    } else {
        return Err("download base must use https, http, or file".into());
    };
    super::super::output(
        Command::new("curl")
            .args([
                "--disable",
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--connect-timeout",
                "20",
                "--max-time",
                "300",
                "--max-filesize",
                &MAX_ARCHIVE.to_string(),
                "--proto",
                protocols,
                "--proto-redir",
                protocols,
                "--output",
            ])
            .arg(destination)
            .arg("--url")
            .arg(url),
        "registry download",
    )?;
    Ok(())
}
fn verify_archive(path: &Path, package: &Locked) -> Result<()> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_ARCHIVE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_ARCHIVE {
        return Err("registry archive exceeds 128 MiB".into());
    }
    if hex(&bytes)
        != package
            .checksum
            .as_deref()
            .ok_or("missing registry checksum")?
    {
        return Err(format!(
            "checksum mismatch for {} {} ({})",
            package.name,
            package.version,
            path.display()
        ));
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(64);
    const HEX: &[u8] = b"0123456789abcdef";
    for byte in digest {
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 15) as usize] as char);
    }
    text
}
fn git_identity(source: &str) -> Result<(&str, &str)> {
    let (url, commit) = source
        .strip_prefix("git+")
        .and_then(|s| s.rsplit_once('#'))
        .ok_or("unsupported locked source; expected crates.io or git with an exact commit")?;
    let url = url.split('?').next().ok_or("missing git URL")?;
    if !["https://", "ssh://", "file://"]
        .iter()
        .any(|p| url.starts_with(p))
        || commit.len() != 40
        || !commit.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("expected an https, ssh, or file git URL and a full locked commit".into());
    }
    Ok((url, commit))
}
fn git(home: &Path, source: &str, offline: bool) -> Result<()> {
    let (url, commit) = git_identity(source)?;
    if offline {
        return Err(format!("offline git checkout missing for {commit}"));
    }
    let parent = home
        .join("git/checkouts")
        .join(format!("flux-{}", hex(url.as_bytes())));
    let destination = parent.join(commit);
    if destination.exists() {
        return Err(format!(
            "incomplete git checkout at {}",
            destination.display()
        ));
    }
    let temporary = Temporary::new(&parent)?;
    let checkout = temporary.0.join("checkout");
    // Planners ignore temporary directories and only inspect completed checkouts.
    git_command(Command::new("git").args(["init", "--quiet"]).arg(&checkout))?;
    git_command(
        Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(["remote", "add", "origin", url]),
    )?;
    git_command(Command::new("git").arg("-C").arg(&checkout).args([
        "fetch",
        "--quiet",
        "--depth=1",
        "origin",
        commit,
    ]))?;
    git_command(
        Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(["checkout", "--quiet", "--detach", commit]),
    )?;
    let actual = git_command(
        Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(["rev-parse", "HEAD"]),
    )?;
    if actual.trim() != commit {
        return Err("git checkout differs from the locked commit".into());
    }
    git_command(Command::new("git").arg("-C").arg(&checkout).args([
        "submodule",
        "update",
        "--init",
        "--recursive",
        "--depth=1",
    ]))?;
    if let Err(error) = std::fs::rename(&checkout, &destination) {
        let actual = git_command(
            Command::new("git")
                .arg("-C")
                .arg(&destination)
                .args(["rev-parse", "HEAD"]),
        )?;
        if actual.trim() != commit {
            return Err(format!("cannot install git checkout: {error}"));
        }
    }
    eprintln!("fetched git commit {commit}");
    Ok(())
}
fn git_command(command: &mut Command) -> Result<String> {
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_HTTP_LOW_SPEED_LIMIT", "1")
        .env("GIT_HTTP_LOW_SPEED_TIME", "60");
    super::super::output(command, "locked git fetch")
}
fn validate_manifest(path: &Path, package: &Locked) -> Result<()> {
    let table = super::read_table(path)?;
    let fields = table
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or("fetched manifest is missing [package]")?;
    let name = super::string(fields, "name")?;
    let version = super::package_version(fields, path)?;
    if name != package.name || version != package.version {
        return Err(format!(
            "fetched manifest differs from locked {} {}",
            package.name, package.version
        ));
    }
    Ok(())
}
struct Temporary(PathBuf);
impl Temporary {
    fn new(parent: &Path) -> Result<Self> {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = parent.join(format!(".flux-fetch-{}-{id}", std::process::id()));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
