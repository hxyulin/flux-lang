//! Reuses a build script's stdout and OUT_DIR after checking its inputs.

use crate::artifact_cache::hash_file;
use crate::flib::executable::Stamp;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

type Digest = [u8; 32];

#[derive(Default)]
pub(super) struct Rerun {
    pub(super) paths: BTreeSet<PathBuf>,
    pub(super) environment: BTreeSet<String>,
}

#[derive(Deserialize, Serialize)]
struct Record {
    version: u32,
    command: Digest,
    stdout: Digest,
    generated: Digest,
    inputs: BTreeMap<PathBuf, Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    package: Option<Digest>,
    environment: BTreeMap<String, Digest>,
    #[serde(default, skip_serializing_if = "FileHashes::is_empty")]
    file_hashes: FileHashes,
}

#[derive(Deserialize, Serialize)]
struct FileHash {
    stamp: Stamp,
    digest: Digest,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(transparent)]
struct FileHashes {
    entries: BTreeMap<PathBuf, FileHash>,
    #[serde(skip)]
    changed: bool,
}

impl FileHashes {
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn digest(&mut self, path: &Path, metadata: &fs::Metadata) -> Option<Digest> {
        let stamp = Stamp::of(metadata);
        // Small files cost little to hash and can outnumber useful cache hits.
        if metadata.len() < 4096 || !stamp.settled() {
            return hash_file(path).ok();
        }
        if let Some(entry) = self.entries.get(path)
            && entry.stamp == stamp
        {
            return Some(entry.digest);
        }
        let digest = hash_file(path).ok()?;
        self.entries
            .insert(path.to_owned(), FileHash { stamp, digest });
        self.changed = true;
        Some(digest)
    }
}

pub(super) struct RunCache<'a> {
    directory: &'a Path,
    out: &'a Path,
}

impl<'a> RunCache<'a> {
    pub(super) fn new(directory: &'a Path, out: &'a Path) -> Self {
        Self { directory, out }
    }

    fn path(&self) -> PathBuf {
        self.directory.join("script-fresh.json")
    }

    pub(super) fn invalidate(&self) -> io::Result<()> {
        match fs::remove_file(self.path()) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    pub(super) fn fresh_stdout(&self, command: &Command) -> Option<String> {
        let mut record: Record = serde_json::from_slice(&fs::read(self.path()).ok()?).ok()?;
        if record.version != 1
            || command_key(command, None, &mut record.file_hashes)? != record.command
            || record.package.is_some_and(|digest| {
                package_fingerprint(command, None, &mut record.file_hashes) != Some(digest)
            })
            || record.inputs.iter().any(|(path, &digest)| {
                fingerprint(path, true, None, &mut record.file_hashes) != Some(digest)
            })
            || record
                .environment
                .iter()
                .any(|(name, &digest)| environment(command, name) != digest)
            || fingerprint(self.out, false, None, &mut record.file_hashes)? != record.generated
        {
            return None;
        }
        let stdout = fs::read_to_string(self.directory.join("stdout")).ok()?;
        if blake3::hash(stdout.as_bytes()).as_bytes() != &record.stdout {
            return None;
        }
        if record.file_hashes.changed
            && let Ok(bytes) = serde_json::to_vec(&record)
        {
            let _ = crate::flib::write_cache(&self.path(), &bytes);
        }
        Some(stdout)
    }

    pub(super) fn record(
        &self,
        command: &Command,
        rerun: &Rerun,
        stdout: &str,
        started: SystemTime,
    ) {
        if let Some(record) = self.snapshot(command, rerun, stdout, started)
            && let Ok(bytes) = serde_json::to_vec(&record)
        {
            // The script already ran successfully. A cache write failure
            // only means the next invocation has to run it again.
            let _ = crate::flib::write_atomic(&self.path(), &bytes);
        }
    }

    fn snapshot(
        &self,
        command: &Command,
        rerun: &Rerun,
        stdout: &str,
        started: SystemTime,
    ) -> Option<Record> {
        let directory = command.get_current_dir()?;
        let mut file_hashes = FileHashes::default();
        let package = if rerun.paths.is_empty() && rerun.environment.is_empty() {
            Some(package_fingerprint(
                command,
                Some(started),
                &mut file_hashes,
            )?)
        } else {
            None
        };
        let inputs = rerun
            .paths
            .iter()
            .map(|path| {
                if path.as_os_str().is_empty() {
                    return None;
                }
                let path = directory.join(path);
                let digest = fingerprint(&path, true, Some(started), &mut file_hashes)?;
                Some((path, digest))
            })
            .collect::<Option<_>>()?;
        let environment = rerun
            .environment
            .iter()
            .map(|name| (name.clone(), environment(command, name)))
            .collect();
        Some(Record {
            version: 1,
            command: command_key(command, Some(started), &mut file_hashes)?,
            stdout: blake3::hash(stdout.as_bytes()).into(),
            generated: fingerprint(self.out, false, None, &mut file_hashes)?,
            inputs,
            package,
            environment,
            file_hashes,
        })
    }
}

fn environment(command: &Command, name: &str) -> Digest {
    let value = match command.get_envs().find(|(key, _)| *key == name) {
        Some((_, value)) => value.map(OsStr::to_owned),
        None => std::env::var_os(name),
    };
    let mut hash = blake3::Hasher::new();
    optional_value(&mut hash, value.as_deref());
    hash.finalize().into()
}

fn command_key(
    command: &Command,
    started: Option<SystemTime>,
    file_hashes: &mut FileHashes,
) -> Option<Digest> {
    let mut hash = blake3::Hasher::new();
    hash.update(&fingerprint(
        Path::new(command.get_program()),
        true,
        started,
        file_hashes,
    )?);
    field(&mut hash, command.get_current_dir()?.as_os_str());
    // Profile, features and DEP_* metadata are implicit inputs. Inherited
    // variables are checked separately when the script declares them.
    for (name, value) in command.get_envs() {
        field(&mut hash, name);
        optional_value(&mut hash, value);
    }
    Some(hash.finalize().into())
}

fn optional_value(hash: &mut blake3::Hasher, value: Option<&OsStr>) {
    hash.update(&[u8::from(value.is_some())]);
    if let Some(value) = value {
        field(hash, value);
    }
}

fn field(hash: &mut blake3::Hasher, field: &OsStr) {
    let bytes = field.as_encoded_bytes();
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn package_fingerprint(
    command: &Command,
    started: Option<SystemTime>,
    file_hashes: &mut FileHashes,
) -> Option<Digest> {
    let mut hash = blake3::Hasher::new();
    hash.update(&fingerprint_tree(
        command.get_current_dir()?,
        true,
        started,
        file_hashes,
        true,
    )?);
    // Preserve freshness when an inherited input changes, even when the
    // script omitted its rerun-if-env-changed declaration.
    let mut inherited: BTreeMap<_, _> = std::env::vars_os().collect();
    for (name, _) in command.get_envs() {
        inherited.remove(name);
    }
    for (name, value) in inherited {
        field(&mut hash, &name);
        field(&mut hash, &value);
    }
    Some(hash.finalize().into())
}

fn fingerprint(
    path: &Path,
    watch: bool,
    started: Option<SystemTime>,
    file_hashes: &mut FileHashes,
) -> Option<Digest> {
    fingerprint_tree(path, watch, started, file_hashes, false)
}

fn fingerprint_tree(
    path: &Path,
    watch: bool,
    started: Option<SystemTime>,
    file_hashes: &mut FileHashes,
    package_root: bool,
) -> Option<Digest> {
    fn visit(
        path: &Path,
        watch: bool,
        started: Option<SystemTime>,
        hash: &mut blake3::Hasher,
        file_hashes: &mut FileHashes,
        package_root: bool,
    ) -> Option<()> {
        let metadata = fs::symlink_metadata(path).ok()?;
        let before = Stamp::of(&metadata);
        // The first declaration arrives after execution. Decline to cache
        // an input that may have changed before the script finished reading it.
        if started.is_some_and(|time| !before.changed_before(time)) {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            hash.update(&metadata.permissions().mode().to_le_bytes());
        }
        if watch {
            let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
            hash.update(&modified.as_secs().to_le_bytes());
            hash.update(&modified.subsec_nanos().to_le_bytes());
        }
        if metadata.is_file() {
            hash.update(b"f");
            hash.update(&file_hashes.digest(path, &metadata)?);
        } else if metadata.is_dir() {
            hash.update(b"d");
            let mut children = fs::read_dir(path)
                .ok()?
                .map(|child| child.map(|entry| entry.file_name()))
                .collect::<io::Result<Vec<_>>>()
                .ok()?;
            children.sort_unstable();
            for name in children {
                let child = path.join(&name);
                // Cargo's default package scan excludes VCS metadata and the
                // top-level build output directory. Include other files
                // conservatively, even when the manifest excludes them.
                if package_root
                    && (name == ".git"
                        || (name == "target" && fs::symlink_metadata(&child).ok()?.is_dir()))
                {
                    continue;
                }
                field(hash, &name);
                visit(&child, watch, started, hash, file_hashes, false)?;
            }
            hash.update(b"e");
        } else {
            // Symlinks and special files can observe state outside this tree.
            return None;
        }
        (Stamp::of(&fs::symlink_metadata(path).ok()?) == before).then_some(())
    }

    let mut hash = blake3::Hasher::new();
    field(&mut hash, fs::canonicalize(path).ok()?.as_os_str());
    visit(path, watch, started, &mut hash, file_hashes, package_root)?;
    Some(hash.finalize().into())
}
