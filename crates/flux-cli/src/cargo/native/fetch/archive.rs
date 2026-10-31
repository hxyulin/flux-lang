//! Bounded extraction of crate archives. Links and special files are never installed.
use super::{Result, hex};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MAX_UNPACKED: usize = 512 * 1024 * 1024;
const MAX_FILES: usize = 100_000;
const BLOCK: usize = 512;

pub(super) fn unpack(
    path: &Path,
    destination: &Path,
    package: &str,
) -> Result<BTreeMap<String, String>> {
    // gzip handles compression and CRC checks; Rust validates every tar entry before writing it.
    let mut child = Command::new("gzip")
        .args(["-dc", "--"])
        .arg(path)
        .env_remove("GZIP")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot decompress crate archive: {e}"))?;
    let mut bytes = Vec::new();
    let result = child
        .stdout
        .take()
        .ok_or("missing gzip output")?
        .take(MAX_UNPACKED as u64 + 1)
        .read_to_end(&mut bytes);
    if result.is_err() || bytes.len() > MAX_UNPACKED {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    result.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_UNPACKED {
        return Err("crate archive exceeds 512 MiB uncompressed".into());
    }
    if !status.success() {
        return Err("invalid gzip crate archive".into());
    }
    extract(&bytes, destination, package)
}
fn extract(bytes: &[u8], destination: &Path, package: &str) -> Result<BTreeMap<String, String>> {
    let mut cursor = 0;
    let mut count = 0;
    let mut pending_path = None;
    let mut pending_size = None;
    let mut files = BTreeMap::new();
    loop {
        let header = bytes
            .get(cursor..cursor + BLOCK)
            .ok_or("truncated crate archive header")?;
        cursor += BLOCK;
        if header.iter().all(|b| *b == 0) {
            if pending_path.is_some() || pending_size.is_some() {
                return Err("unfinished archive extension header".into());
            }
            if bytes.len() < cursor + BLOCK || bytes[cursor..].iter().any(|b| *b != 0) {
                return Err("invalid crate archive terminator".into());
            }
            return Ok(files);
        }
        count += 1;
        if count > MAX_FILES {
            return Err("crate archive has too many entries".into());
        }
        let expected = number(&header[148..156])?;
        let checksum: u64 = header
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    *b as u64
                }
            })
            .sum();
        if expected != checksum {
            return Err("invalid tar header checksum".into());
        }
        let kind = header[156];
        let size = if matches!(kind, b'L' | b'x') {
            number(&header[124..136])?
        } else {
            pending_size.take().unwrap_or(number(&header[124..136])?)
        };
        if size > MAX_UNPACKED as u64 {
            return Err("crate archive entry is too large".into());
        }
        let size = size as usize;
        let data = bytes
            .get(cursor..cursor + size)
            .ok_or("truncated crate archive entry")?;
        cursor = cursor
            .checked_add(size.div_ceil(BLOCK) * BLOCK)
            .ok_or("archive size overflow")?;
        if cursor > bytes.len() {
            return Err("truncated crate archive padding".into());
        }
        if kind == b'L' {
            if data.len() > 4096 {
                return Err("archive path is too long".into());
            }
            pending_path = Some(text(data)?.trim_end_matches('\0').to_owned());
            continue;
        }
        if kind == b'x' {
            if data.len() > 64 * 1024 {
                return Err("archive extension header is too large".into());
            }
            for (key, value) in pax(data)? {
                match key {
                    "path" => pending_path = Some(value.into()),
                    "size" => pending_size = Some(value.parse::<u64>().map_err(|e| e.to_string())?),
                    "mtime" | "atime" | "ctime" | "uid" | "gid" | "uname" | "gname" | "comment"
                    | "charset" | "hdrcharset" => {}
                    _ => return Err(format!("unsupported crate archive extension {key}")),
                }
            }
            continue;
        }
        if !matches!(kind, 0 | b'0' | b'5') {
            return Err("crate archive links and special entries are not supported".into());
        }
        let name = if let Some(path) = pending_path.take() {
            path
        } else {
            let name = field(&header[..100])?;
            if &header[257..263] == b"ustar\0" {
                let prefix = field(&header[345..500])?;
                if prefix.is_empty() {
                    name.into()
                } else {
                    format!("{prefix}/{name}")
                }
            } else {
                name.into()
            }
        };
        let relative = relative(&name, package)?;
        let path = destination.join(package).join(&relative);
        if kind == b'5' {
            if size != 0 {
                return Err("directory archive entry has data".into());
            }
            std::fs::create_dir_all(&path)
                .map_err(|e| format!("cannot create archive directory: {e}"))?;
        } else {
            if relative.as_os_str().is_empty() {
                return Err("crate archive root must be a directory".into());
            }
            let key = relative.to_string_lossy().into_owned();
            if matches!(key.as_str(), ".cargo-checksum.json" | ".cargo-ok") {
                return Err("reserved crate cache metadata in archive".into());
            }
            if files.contains_key(&key) {
                return Err("duplicate crate archive file".into());
            }
            std::fs::create_dir_all(path.parent().ok_or("archive entry has no parent")?)
                .map_err(|e| e.to_string())?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| format!("cannot create archive file: {e}"))?;
            file.write_all(data).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if number(&header[100..108])? & 0o111 != 0 {
                    0o755
                } else {
                    0o644
                };
                file.set_permissions(std::fs::Permissions::from_mode(mode))
                    .map_err(|e| e.to_string())?;
            }
            files.insert(key, hex(data));
        }
    }
}
fn relative(name: &str, package: &str) -> Result<PathBuf> {
    if name.len() > 4096 || name.contains(['\\', '\0', ':', '\n', '\r']) {
        return Err("invalid crate archive path".into());
    }
    let mut components = name.trim_end_matches('/').split('/');
    if components.next() != Some(package) {
        return Err("crate archive entry is outside its package root".into());
    }
    let mut path = PathBuf::new();
    for component in components {
        if matches!(component, "" | "." | "..") {
            return Err("invalid crate archive path component".into());
        }
        path.push(component);
    }
    Ok(path)
}
fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|e| format!("non-UTF-8 crate archive path: {e}"))
}
fn field(bytes: &[u8]) -> Result<&str> {
    text(bytes.split(|b| *b == 0).next().unwrap_or_default())
}
fn number(bytes: &[u8]) -> Result<u64> {
    let value = text(bytes)?.trim_matches(['\0', ' ']);
    if value.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(value, 8).map_err(|e| format!("invalid tar number: {e}"))
}
fn pax(mut data: &[u8]) -> Result<Vec<(&str, &str)>> {
    let mut result = Vec::new();
    while !data.is_empty() {
        let space = data
            .iter()
            .position(|b| *b == b' ')
            .ok_or("invalid archive extension record")?;
        let len = text(&data[..space])?
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        let record = data
            .get(space + 1..len)
            .ok_or("invalid archive extension length")?;
        let record = text(record)?
            .strip_suffix('\n')
            .ok_or("invalid archive extension terminator")?;
        let (key, value) = record
            .split_once('=')
            .ok_or("invalid archive extension field")?;
        result.push((key, value));
        data = &data[len..];
    }
    Ok(result)
}
