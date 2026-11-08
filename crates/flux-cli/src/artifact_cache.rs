//! Executable freshness records, using the compiler's observed input snapshots.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::flib::freshness::Snapshot;

#[derive(Deserialize, Serialize)]
struct Record {
    version: u32,
    output: [u8; 32],
    snapshot: Option<Vec<u8>>,
    semantic: [u8; 32],
    native: crate::native_link::Snapshot,
}

fn path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_owned();
    path.push(".fresh.json");
    PathBuf::from(path)
}

pub(crate) fn record(
    output: &Path,
    snapshot: Option<&Snapshot>,
    semantic: [u8; 32],
    native: Option<crate::native_link::Snapshot>,
) -> Result<(), String> {
    let Some(native) = native else {
        // An untracked link must not inherit an older tracked result.
        match std::fs::remove_file(path(output)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        return Ok(());
    };
    let record = Record {
        version: 2,
        output: hash_file(output)?,
        snapshot: snapshot.map(Snapshot::to_bytes).transpose()?,
        semantic,
        native,
    };
    crate::flib::write_atomic(
        &path(output),
        &serde_json::to_vec(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn read(output: &Path) -> Option<Record> {
    let bytes = std::fs::read(path(output)).ok()?;
    let record: Record = serde_json::from_slice(&bytes).ok()?;
    (record.version == 2).then_some(record)
}

pub(crate) fn is_fresh(
    output: &Path,
    configuration: &[u8],
    dependencies: &[[u8; 32]],
    environment: Option<&BTreeMap<String, String>>,
) -> Result<bool, String> {
    let Some(record) = read(output) else {
        return Ok(false);
    };
    let Some(bytes) = &record.snapshot else {
        return Ok(false);
    };
    let Ok(snapshot) = Snapshot::from_bytes(bytes) else {
        return Ok(false);
    };
    if !snapshot.is_fresh(configuration, dependencies, environment)? {
        return Ok(false);
    }
    Ok(record.native.is_fresh() && hash_file(output).ok() == Some(record.output))
}

/// Called only after current macro expansion and HIR diagnostics have completed.
pub(crate) fn is_semantically_fresh(
    output: &Path,
    semantic: [u8; 32],
    native: &flux_codegen::NativeLink,
) -> bool {
    read(output).is_some_and(|record| {
        record.semantic == semantic
            && record.native.matches_request(native)
            && record.native.is_fresh()
            && hash_file(output).ok() == Some(record.output)
    })
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
