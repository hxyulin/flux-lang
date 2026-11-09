//! Fluxlib archive framing and deterministic content identities.

use std::fmt;

const ARCHIVE_MAGIC: &[u8; 8] = b"!<arch>\n";
const META_MAGIC: &[u8; 8] = b"FLUXMETA";
const VERSION: u32 = 2;
const MAX_METADATA: usize = 512 * 1024 * 1024;

/// Opt-in metadata timings include nested work; phases must not be summed.
pub(crate) fn time<T>(phase: &str, f: impl FnOnce() -> T) -> T {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let enabled = *ENABLED.get_or_init(|| std::env::var_os("FLUX_METADATA_TIMINGS").is_some());
    let start = enabled.then(std::time::Instant::now);
    let result = f();
    if let Some(start) = start {
        eprintln!(
            "metadata timing: {phase}: {:.3} ms (inclusive)",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
    result
}

/// An invalid or incompatible Fluxlib artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(&'static str);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}

/// A section of the private metadata payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub tag: u32,
    pub bytes: Vec<u8>,
}

/// The complete metadata envelope, independent of process-local IR handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub compiler: [u8; 32],
    pub sections: Vec<Section>,
}

impl Metadata {
    /// Encodes canonical section order, rejecting duplicate or unsupported tags.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let mut sections: Vec<_> = self.sections.iter().collect();
        sections.sort_by_key(|s| s.tag);
        if sections.len() != 9
            || sections.windows(2).any(|s| s[0].tag == s[1].tag)
            || sections.iter().any(|s| !(1..=9).contains(&s.tag))
        {
            return Err(Error("invalid metadata sections"));
        }
        let mut offset = 8 + 4 + 4 + 32 + 4 + sections.len() * 20;
        let mut out = Vec::new();
        out.extend_from_slice(META_MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&32u32.to_le_bytes());
        out.extend_from_slice(&self.compiler);
        out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
        for section in &sections {
            out.extend_from_slice(&section.tag.to_le_bytes());
            out.extend_from_slice(&(offset as u64).to_le_bytes());
            out.extend_from_slice(&(section.bytes.len() as u64).to_le_bytes());
            offset = offset
                .checked_add(section.bytes.len())
                .ok_or(Error("metadata is too large"))?;
        }
        if offset
            .checked_add(32)
            .is_none_or(|total| total > MAX_METADATA)
        {
            return Err(Error("metadata is too large"));
        }
        for section in sections {
            out.extend_from_slice(&section.bytes);
        }
        let checksum = time("metadata checksum write", || flux_hir::flib::digest(&out));
        out.extend_from_slice(&checksum);
        Ok(out)
    }

    /// Checks framing and compiler identity before copying section payloads.
    pub fn decode(bytes: &[u8], compiler: [u8; 32]) -> Result<Self, Error> {
        let index = MetadataIndex::decode(bytes, compiler)?;
        Ok(Self {
            compiler,
            sections: index
                .sections
                .iter()
                .map(|s| Section {
                    tag: s.tag,
                    bytes: bytes[s.bytes.clone()].to_vec(),
                })
                .collect(),
        })
    }
}

#[derive(Debug)]
struct SectionRange {
    tag: u32,
    bytes: std::ops::Range<usize>,
}

/// Checked section ranges, retained with the immutable metadata they describe.
#[derive(Debug)]
pub(crate) struct MetadataIndex {
    sections: Vec<SectionRange>,
}

impl MetadataIndex {
    pub(crate) fn section<'a>(&self, bytes: &'a [u8], tag: u32) -> Option<&'a [u8]> {
        self.sections
            .iter()
            .find(|s| s.tag == tag)
            .map(|s| &bytes[s.bytes.clone()])
    }
    pub(crate) fn decode(bytes: &[u8], compiler: [u8; 32]) -> Result<Self, Error> {
        if bytes.len() < 84 || bytes.len() > MAX_METADATA || &bytes[..8] != META_MAGIC {
            return Err(Error("invalid flux metadata header"));
        }
        if read_u32(bytes, 8)? != VERSION {
            return Err(Error("unsupported flux metadata version"));
        }
        if read_u32(bytes, 12)? != 32 || bytes[16..48] != compiler {
            return Err(Error(
                "flux compiler identity mismatch; rebuild the artifact",
            ));
        }
        let payload_end = bytes.len() - 32;
        if time("metadata checksum", || {
            flux_hir::flib::digest(&bytes[..payload_end])
        }) != bytes[payload_end..]
        {
            return Err(Error(
                "flux metadata checksum mismatch; rebuild the artifact",
            ));
        }
        let bytes = &bytes[..payload_end];
        let count = read_u32(bytes, 48)? as usize;
        if count != 9 {
            return Err(Error("invalid metadata sections"));
        }
        let mut end = 52 + count * 20;
        if end > bytes.len() {
            return Err(Error("truncated metadata directory"));
        }
        let mut sections = Vec::new();
        let mut previous = 0;
        for i in 0..count {
            let at = 52 + i * 20;
            let tag = read_u32(bytes, at)?;
            if tag <= previous || tag > 9 {
                return Err(Error("invalid metadata section tag"));
            }
            previous = tag;
            let start = read_size(bytes, at + 4)?;
            let len = read_size(bytes, at + 12)?;
            if start != end {
                return Err(Error("invalid metadata section range"));
            }
            end = start
                .checked_add(len)
                .ok_or(Error("invalid metadata section range"))?;
            bytes
                .get(start..end)
                .ok_or(Error("truncated metadata section"))?;
            sections.push(SectionRange {
                tag,
                bytes: start..end,
            });
        }
        if end != bytes.len() {
            return Err(Error("trailing metadata bytes"));
        }
        Ok(Self { sections })
    }
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, Error> {
    let raw = bytes
        .get(at..at + 4)
        .ok_or(Error("truncated metadata integer"))?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}
fn read_size(bytes: &[u8], at: usize) -> Result<usize, Error> {
    let raw: [u8; 8] = bytes
        .get(at..at + 8)
        .ok_or(Error("truncated metadata integer"))?
        .try_into()
        .map_err(|_| Error("invalid metadata integer"))?;
    usize::try_from(u64::from_le_bytes(raw)).map_err(|_| Error("metadata integer is too large"))
}

/// A metadata-only or linkable Fluxlib archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archive {
    pub metadata: Vec<u8>,
    pub object: Option<Vec<u8>>,
}

impl Archive {
    /// Writes a regular ar archive with deterministic headers and short names.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let mut out = ARCHIVE_MAGIC.to_vec();
        member(&mut out, "flux.meta/", &self.metadata)?;
        if let Some(object) = &self.object {
            member(&mut out, "flux.o/", object)?;
        }
        Ok(out)
    }

    /// Reads exactly one metadata member and at most one native object.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let index = ArchiveIndex::decode(bytes)?;
        Ok(Self {
            metadata: index.metadata(bytes).to_vec(),
            object: index.object(bytes).map(<[u8]>::to_vec),
        })
    }
}

/// Checked archive ranges; payloads stay in the caller's byte buffer.
#[derive(Debug)]
pub(crate) struct ArchiveIndex {
    metadata: std::ops::Range<usize>,
    object: Option<std::ops::Range<usize>>,
}

impl ArchiveIndex {
    pub(crate) fn metadata<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        &bytes[self.metadata.clone()]
    }

    pub(crate) fn object<'a>(&self, bytes: &'a [u8]) -> Option<&'a [u8]> {
        self.object.as_ref().map(|range| &bytes[range.clone()])
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.get(..8) != Some(ARCHIVE_MAGIC.as_slice()) {
            return Err(Error("invalid fluxlib archive"));
        }
        let mut at: usize = 8;
        let mut metadata = None;
        let mut object = None;
        while at < bytes.len() {
            let header = bytes
                .get(at..at.checked_add(60).ok_or(Error("invalid archive offset"))?)
                .ok_or(Error("truncated archive header"))?;
            if &header[58..] != b"`\n" {
                return Err(Error("invalid archive header"));
            }
            let name = std::str::from_utf8(&header[..16])
                .map_err(|_| Error("invalid archive name"))?
                .trim_end();
            let size: usize = std::str::from_utf8(&header[48..58])
                .map_err(|_| Error("invalid archive size"))?
                .trim()
                .parse()
                .map_err(|_| Error("invalid archive size"))?;
            at += 60;
            let end = at.checked_add(size).ok_or(Error("invalid archive size"))?;
            bytes
                .get(at..end)
                .ok_or(Error("truncated archive member"))?;
            match name {
                "flux.meta/" if metadata.is_none() => {
                    if size > MAX_METADATA {
                        return Err(Error("metadata is too large"));
                    }
                    metadata = Some(at..end);
                }
                "flux.o/" if object.is_none() => object = Some(at..end),
                _ => return Err(Error("unsupported or duplicate fluxlib member")),
            }
            at = end;
            if !size.is_multiple_of(2) {
                if bytes.get(at) != Some(&b'\n') {
                    return Err(Error("invalid archive padding"));
                }
                at += 1;
            }
        }
        Ok(Self {
            metadata: metadata.ok_or(Error("missing flux metadata"))?,
            object,
        })
    }
}

fn member(out: &mut Vec<u8>, name: &str, bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > 9_999_999_999 {
        return Err(Error("archive member is too large"));
    }
    let header = format!(
        "{name:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
        0,
        0,
        0,
        "100644",
        bytes.len()
    );
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(bytes);
    if !bytes.len().is_multiple_of(2) {
        out.push(b'\n');
    }
    Ok(())
}

static COMPILER_ID: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

/// Identifies this compiler executable, including its linked backend and embedded libraries.
/// An unreadable executable disables artifact creation/loading rather than guessing identity.
pub fn compiler_identity() -> Result<[u8; 32], String> {
    if let Some(id) = COMPILER_ID.get() {
        return Ok(*id);
    }
    let path =
        std::env::current_exe().map_err(|e| format!("cannot identify flux compiler: {e}"))?;
    let id = time("compiler executable fingerprint", || {
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("cannot fingerprint {}: {e}", path.display()))?;
        Ok::<_, String>(flux_hir::flib::digest(&bytes))
    })?;
    Ok(*COMPILER_ID.get_or_init(|| id))
}

pub(crate) fn checked_compiler_identity(hash: [u8; 32]) -> Result<(), String> {
    if *COMPILER_ID.get_or_init(|| hash) != hash {
        return Err("compiler identity changed within one process".into());
    }
    Ok(())
}

pub mod semantic;

/// Replaces an artifact only after its complete contents have been written.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let (temporary, mut file) = loop {
        let name = format!(
            ".flux-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let temporary = parent.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    };
    struct Remove(std::path::PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let guard = Remove(temporary);
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&guard.0, path)
}

pub mod executable;
pub mod freshness;
