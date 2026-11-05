//! Executable identity with a stat fast path for a staged compiler suite.

use std::io::Read;
use std::path::Path;

use flux_hir::flib::{Decode, Decoder, Encode, Encoder, Error, digest};

/// The stat fields that change whenever a file's content is replaced or written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    modified: i64,
    modified_ns: i64,
    changed: i64,
    changed_ns: i64,
}

impl Stamp {
    #[cfg(unix)]
    pub(crate) fn of(metadata: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified: metadata.mtime(),
            modified_ns: metadata.mtime_nsec(),
            changed: metadata.ctime(),
            changed_ns: metadata.ctime_nsec(),
        }
    }

    /// Whether the last change is old enough that a later write must give a
    /// different change time. A coarse file-system clock can give two writes
    /// within one tick the same stamp, so recent files are not trusted.
    pub(crate) fn settled(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        cfg!(unix) && now - self.changed >= 2
    }

    #[cfg(not(unix))]
    pub(crate) fn of(metadata: &std::fs::Metadata) -> Self {
        Self {
            device: 0,
            inode: 0,
            size: metadata.len(),
            modified: 0,
            modified_ns: 0,
            changed: 0,
            changed_ns: 0,
        }
    }
}

/// The fingerprint and executable stat fields recorded when a suite is staged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableIdentity {
    stamp: Stamp,
    hash: [u8; 32],
}

/// How startup obtained the current executable identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMethod {
    Stat,
    Hash,
}

/// A current identity, ready to compare with the suite's recorded fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckedIdentity {
    pub hash: [u8; 32],
    pub method: CheckMethod,
}

fn fingerprint(path: &Path) -> Result<ExecutableIdentity, String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let before = Stamp::of(&file.metadata().map_err(|e| e.to_string())?);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("cannot fingerprint {}: {e}", path.display()))?;
    let after = Stamp::of(&file.metadata().map_err(|e| e.to_string())?);
    if before != after || bytes.len() as u64 != after.size {
        return Err(format!(
            "executable changed while fingerprinting {}",
            path.display()
        ));
    }
    Ok(ExecutableIdentity {
        stamp: after,
        hash: digest(&bytes),
    })
}

impl ExecutableIdentity {
    /// Checks stat fields for an already opened file without reading its content.
    pub fn matches_metadata(&self, metadata: &std::fs::Metadata) -> bool {
        cfg!(unix) && Stamp::of(metadata) == self.stamp
    }

    /// Records the complete executable content and its stat fields.
    pub fn capture(path: &Path) -> Result<Self, String> {
        fingerprint(path)
    }

    /// The SHA-256 recorded at staging.
    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }

    /// Uses the recorded hash only when every executable stat field still matches.
    /// A copy, metadata change or rebuild takes the complete content-hash path.
    pub fn check(&self, path: &Path) -> Result<CheckedIdentity, String> {
        let metadata =
            std::fs::metadata(path).map_err(|e| format!("cannot stat {}: {e}", path.display()))?;
        if self.matches_metadata(&metadata) {
            return Ok(CheckedIdentity {
                hash: self.hash,
                method: CheckMethod::Stat,
            });
        }
        let current = fingerprint(path)?;
        Ok(CheckedIdentity {
            hash: current.hash,
            method: CheckMethod::Hash,
        })
    }
}

impl Encode for ExecutableIdentity {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.stamp.device.encode(e)?;
        self.stamp.inode.encode(e)?;
        self.stamp.size.encode(e)?;
        self.stamp.modified.encode(e)?;
        self.stamp.modified_ns.encode(e)?;
        self.stamp.changed.encode(e)?;
        self.stamp.changed_ns.encode(e)?;
        self.hash.as_slice().encode(e)
    }
}
impl Decode for ExecutableIdentity {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let stamp = Stamp {
            device: Decode::decode(d)?,
            inode: Decode::decode(d)?,
            size: Decode::decode(d)?,
            modified: Decode::decode(d)?,
            modified_ns: Decode::decode(d)?,
            changed: Decode::decode(d)?,
            changed_ns: Decode::decode(d)?,
        };
        if !(0..1_000_000_000).contains(&stamp.modified_ns)
            || !(0..1_000_000_000).contains(&stamp.changed_ns)
        {
            return Err(Error::new("invalid executable timestamp"));
        }
        let hash = Vec::<u8>::decode(d)?
            .try_into()
            .map_err(|_| Error::new("invalid executable hash"))?;
        Ok(Self { stamp, hash })
    }
}
