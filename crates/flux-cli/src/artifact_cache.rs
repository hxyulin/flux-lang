//! Executable freshness records, using the compiler's observed input snapshots.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::flib::freshness::Snapshot;

#[derive(Deserialize, Serialize)]
struct Record {
    version: u32,
    output: [u8; 32],
    snapshot: Vec<u8>,
}

fn path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_owned();
    path.push(".fresh.json");
    PathBuf::from(path)
}

pub(crate) fn record(output: &Path, snapshot: Option<&Snapshot>) -> Result<(), String> {
    let Some(snapshot) = snapshot else {
        // A previous tracked build must not authorize an untracked one.
        match std::fs::remove_file(path(output)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        return Ok(());
    };
    let record = Record {
        version: 1,
        output: hash_file(output)?,
        snapshot: snapshot.to_bytes()?,
    };
    crate::flib::write_atomic(
        &path(output),
        &serde_json::to_vec(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub(crate) fn is_fresh(
    output: &Path,
    configuration: &[u8],
    dependencies: &[[u8; 32]],
    environment: Option<&BTreeMap<String, String>>,
) -> Result<bool, String> {
    let Ok(bytes) = std::fs::read(path(output)) else {
        return Ok(false);
    };
    let Ok(record) = serde_json::from_slice::<Record>(&bytes) else {
        return Ok(false);
    };
    if record.version != 1 || hash_file(output).ok() != Some(record.output) {
        return Ok(false);
    }
    let Ok(snapshot) = Snapshot::from_bytes(&record.snapshot) else {
        return Ok(false);
    };
    snapshot.is_fresh(configuration, dependencies, environment)
}

/// Stream large objects rather than allocating a buffer the size of the executable.
pub(crate) fn hash_file(path: &Path) -> Result<[u8; 32], String> {
    use std::io::Read;
    let mut input =
        std::fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut hash = blake3::Hasher::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}
