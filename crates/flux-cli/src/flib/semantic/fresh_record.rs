//! Cached identity and freshness data for a previously verified package archive.

use std::io::Read;
use std::path::{Path, PathBuf};

use flux_hir::flib::{Decode, Decoder, Encode, Encoder, Error};

use crate::flib::{compiler_identity, executable::Stamp, freshness::Snapshot, write_cache};

use super::{FreshArtifact, decode_manifest};

const MAGIC: &[u8; 8] = b"FLIBFRC1";
const LIMIT: usize = 4 * 1024 * 1024;

pub(super) fn path(artifact: &Path) -> PathBuf {
    artifact.with_extension("flib.fresh")
}

pub(super) fn read(path: &Path, stamp: Stamp) -> Option<FreshArtifact> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > LIMIT {
        return None;
    }
    let (body, checksum) = bytes.split_at_checked(bytes.len().checked_sub(32)?)?;
    if blake3::hash(body).as_bytes().as_slice() != checksum {
        return None;
    }
    let mut d = Decoder::new(body);
    if d.read(MAGIC.len()).ok()? != MAGIC
        || d.read(32).ok()? != compiler_identity().ok()?
        || Stamp::decode(&mut d).ok()? != stamp
    {
        return None;
    }
    let linkable = bool::decode(&mut d).ok()?;
    let manifest = decode_manifest(d.blob().ok()?, linkable).ok()?;
    let snapshot = Snapshot::from_bytes(d.blob().ok()?).ok();
    d.finish().ok()?;
    Some(FreshArtifact { manifest, snapshot })
}

pub(super) fn record(path: &Path, stamp: Stamp, identity: &[u8], snapshot: &[u8], linkable: bool) {
    if identity.len().saturating_add(snapshot.len()) > LIMIT {
        return;
    }
    let Ok(compiler) = compiler_identity() else {
        return;
    };
    let encode = || -> Result<Vec<u8>, Error> {
        let mut e = Encoder::new();
        e.write(MAGIC);
        e.write(&compiler);
        stamp.encode(&mut e)?;
        linkable.encode(&mut e)?;
        e.blob(identity)?;
        e.blob(snapshot)?;
        let mut bytes = e.into_bytes();
        bytes.extend_from_slice(blake3::hash(&bytes).as_bytes());
        Ok(bytes)
    };
    if let Ok(bytes) = encode()
        && bytes.len() <= LIMIT
    {
        let _ = write_cache(path, &bytes);
    }
}
