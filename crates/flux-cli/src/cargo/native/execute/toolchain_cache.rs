//! Cached version and sysroot queries for a direct Rustup installation.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::flib::executable::Stamp;
use serde::{Deserialize, Serialize};

const MAX_RECORD: u64 = 256 * 1024;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Inputs {
    rustc: PathBuf,
    files: Vec<(PathBuf, Stamp)>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    inputs: Inputs,
    version: String,
    server: PathBuf,
}

pub(super) struct QueryCache {
    path: PathBuf,
    inputs: Inputs,
    expected_server: PathBuf,
}

impl QueryCache {
    pub(super) fn capture(path: &Path, rustc: &OsStr) -> Option<Self> {
        // Custom loaders can replace the libraries selected by the executable.
        if !cfg!(unix)
            || std::env::vars_os().any(|(key, _)| {
                let key = key.to_string_lossy();
                key.starts_with("DYLD_") || key.starts_with("LD_")
            })
        {
            return None;
        }
        let rustup = std::env::var_os("RUSTUP_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".rustup")))?;
        let toolchains = rustup.join("toolchains").canonicalize().ok()?;
        let rustc = Path::new(rustc);
        let install = rustc.parent()?.parent()?;
        if install.parent()? != toolchains
            || rustc != install.join("bin/rustc")
            || rustc.canonicalize().ok()? != rustc
        {
            return None;
        }
        let expected_server = install.join("libexec/rust-analyzer-proc-macro-srv");
        let inputs = snapshot(rustc, &expected_server)?;
        Some(Self {
            path: path.to_owned(),
            inputs,
            expected_server,
        })
    }

    pub(super) fn read(&self) -> Option<(String, PathBuf)> {
        let mut bytes = Vec::new();
        fs::File::open(&self.path)
            .ok()?
            .take(MAX_RECORD + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() as u64 > MAX_RECORD {
            return None;
        }
        let (checksum, payload) = bytes.split_at_checked(32)?;
        if checksum != blake3::hash(payload).as_bytes() {
            return None;
        }
        let record: Record = serde_json::from_slice(payload).ok()?;
        if record.inputs != self.inputs || record.server != self.expected_server {
            return None;
        }
        Some((record.version, record.server))
    }

    pub(super) fn write(self, version: &str, server: &Path) {
        if server != self.expected_server
            || snapshot(&self.inputs.rustc, server).as_ref() != Some(&self.inputs)
        {
            return;
        }
        let record = Record {
            inputs: self.inputs,
            version: version.to_owned(),
            server: server.to_owned(),
        };
        if let Ok(payload) = serde_json::to_vec(&record)
            && payload.len() as u64 + 32 <= MAX_RECORD
        {
            let mut bytes = blake3::hash(&payload).as_bytes().to_vec();
            bytes.extend(payload);
            let _ = crate::flib::write_cache(&self.path, &bytes);
        }
    }
}

fn snapshot(rustc: &Path, server: &Path) -> Option<Inputs> {
    // A wrapper remains a live query even if it is installed at the usual path.
    let mut magic = [0; 4];
    fs::File::open(rustc).ok()?.read_exact(&mut magic).ok()?;
    if !matches!(magic, [0x7f, b'E', b'L', b'F'] | [0xcf, 0xfa, 0xed, 0xfe]) {
        return None;
    }
    let lib = rustc.parent()?.parent()?.join("lib");
    let mut paths = vec![
        rustc.to_owned(),
        server.to_owned(),
        lib.clone(),
        lib.join("rustlib/multirust-channel-manifest.toml"),
    ];
    for entry in fs::read_dir(&lib).ok()? {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort_unstable();
    let files = paths
        .into_iter()
        .map(|path| {
            let stamp = Stamp::of(&fs::metadata(&path).ok()?);
            stamp.settled().then_some((path, stamp))
        })
        .collect::<Option<_>>()?;
    Some(Inputs {
        rustc: rustc.to_owned(),
        files,
    })
}
