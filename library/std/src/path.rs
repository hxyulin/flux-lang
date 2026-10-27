//! Unix paths, with byte-preserving views and normalized component comparison.
use crate::alloc_crate::borrow::ToOwned;
use crate::alloc_crate::string::String;
use crate::ffi::{OsStr, OsString};
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::hash::{Hash, Hasher};
use core::ops::Deref;

#[repr(transparent)]
pub struct Path { inner: OsStr }
#[derive(Clone, Default)]
pub struct PathBuf { inner: OsString }

// Root, current directory, parent directory, or an ordinary component.
// Byte spans preserve original spelling between the remaining components.
fn component(bytes: &[u8], position: &mut usize) -> Option<(u8, usize, usize)> {
    if *position == 0 && !bytes.is_empty() {
        if bytes[0] == b'/' { *position = 1; return Some((0, 0, 1)); }
        if bytes[0] == b'.' && (bytes.len() == 1 || bytes[1] == b'/') {
            *position = 1; return Some((1, 0, 1));
        }
    }
    loop {
        while *position < bytes.len() && bytes[*position] == b'/' { *position += 1; }
        if *position == bytes.len() { return None; }
        let start = *position;
        while *position < bytes.len() && bytes[*position] != b'/' { *position += 1; }
        let end = *position;
        if end == start + 1 && bytes[start] == b'.' { continue; }
        let kind = if end == start + 2 && bytes[start] == b'.' && bytes[start + 1] == b'.' { 2 } else { 3 };
        return Some((kind, start, end));
    }
}

impl Path {
    pub fn new<S: AsRef<OsStr> + ?Sized>(value: &S) -> &Path {
        let os = value.as_ref();
        // SAFETY: transparent Path has precisely OsStr's representation and
        // accepts every OsStr. Borrow lifetime and byte length are preserved.
        unsafe { &*(os as *const OsStr as *const Path) }
    }
    pub fn as_os_str(&self) -> &OsStr { &self.inner }
    pub fn to_str(&self) -> Option<&str> { self.inner.to_str() }
    pub fn to_string_lossy(&self) -> crate::alloc_crate::borrow::Cow<'_, str> { self.inner.to_string_lossy() }
    pub fn to_path_buf(&self) -> PathBuf { PathBuf { inner: self.inner.to_os_string() } }
    pub fn is_absolute(&self) -> bool { self.inner.as_encoded_bytes().first() == Some(&b'/') }
    pub fn has_root(&self) -> bool { self.is_absolute() }
    pub fn is_relative(&self) -> bool { !self.is_absolute() }
    pub fn parent(&self) -> Option<&Path> {
        let bytes = self.inner.as_encoded_bytes();
        let mut position = 0;
        let mut previous_end = 0;
        let mut parent_end = 0;
        let mut last_kind = 0;
        let mut any = false;
        while let Some((kind, _, end)) = component(bytes, &mut position) {
            parent_end = previous_end;
            previous_end = end;
            last_kind = kind;
            any = true;
        }
        if !any || last_kind == 0 { None } else { Some(Path::new(OsStr::from_unix_bytes(&bytes[..parent_end]))) }
    }
    pub fn strip_prefix<P: AsRef<Path>>(&self, base: P) -> Result<&Path, StripPrefixError> {
        let bytes = self.inner.as_encoded_bytes();
        let base = base.as_ref().inner.as_encoded_bytes();
        let mut position = 0;
        let mut base_position = 0;
        while let Some((kind, start, end)) = component(base, &mut base_position) {
            match component(bytes, &mut position) {
                Some((other, left, right)) if kind == other && bytes[left..right] == base[start..end] => {},
                _ => return Err(StripPrefixError(())),
            }
        }
        let mut start = bytes.len();
        let mut end = bytes.len();
        let mut first = true;
        while let Some((_, left, right)) = component(bytes, &mut position) {
            if first { start = left; first = false; }
            end = right;
        }
        Ok(Path::new(OsStr::from_unix_bytes(&bytes[start..end])))
    }
    pub fn starts_with<P: AsRef<Path>>(&self, base: P) -> bool { self.strip_prefix(base).is_ok() }
    pub fn join<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let mut result = self.to_path_buf(); result.push(path); result
    }
    pub fn display(&self) -> Display<'_> { Display { path: self } }
    pub fn is_file(&self) -> bool { crate::fs::is_file(self) }
    pub fn canonicalize(&self) -> crate::io::Result<PathBuf> { crate::fs::canonicalize(self) }
}
impl PathBuf {
    pub fn new() -> Self { Self::default() }
    pub fn as_path(&self) -> &Path { Path::new(&self.inner) }
    pub fn into_os_string(self) -> OsString { self.inner }
    pub fn push<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        let bytes = path.inner.as_encoded_bytes();
        if path.is_absolute() { self.inner.bytes.clear(); }
        else if !self.inner.bytes.is_empty() && self.inner.bytes.last() != Some(&b'/') { self.inner.bytes.push(b'/'); }
        self.inner.bytes.extend_from_slice(bytes);
    }
}
impl Deref for PathBuf { type Target = Path; fn deref(&self) -> &Path { self.as_path() } }
impl ToOwned for Path { type Owned = PathBuf; fn to_owned(&self) -> PathBuf { self.to_path_buf() } }
impl Borrow<Path> for PathBuf { fn borrow(&self) -> &Path { self.as_path() } }
impl AsRef<Path> for Path { fn as_ref(&self) -> &Path { self } }
impl AsRef<Path> for PathBuf { fn as_ref(&self) -> &Path { self.as_path() } }
impl AsRef<Path> for str { fn as_ref(&self) -> &Path { Path::new(self) } }
impl AsRef<Path> for String { fn as_ref(&self) -> &Path { Path::new(self) } }
impl AsRef<Path> for OsStr { fn as_ref(&self) -> &Path { Path::new(self) } }
impl AsRef<Path> for OsString { fn as_ref(&self) -> &Path { Path::new(self) } }
impl AsRef<OsStr> for Path { fn as_ref(&self) -> &OsStr { self.as_os_str() } }
impl AsRef<OsStr> for PathBuf { fn as_ref(&self) -> &OsStr { self.inner.as_os_str() } }
impl From<&Path> for PathBuf { fn from(value: &Path) -> Self { value.to_path_buf() } }
impl From<&PathBuf> for PathBuf { fn from(value: &PathBuf) -> Self { value.clone() } }
impl From<&str> for PathBuf { fn from(value: &str) -> Self { Self { inner: OsString::from(value) } } }
impl From<String> for PathBuf { fn from(value: String) -> Self { Self { inner: OsString::from(value) } } }
impl From<OsString> for PathBuf { fn from(value: OsString) -> Self { Self { inner: value } } }
impl From<&OsStr> for PathBuf { fn from(value: &OsStr) -> Self { Self { inner: value.to_os_string() } } }

impl PartialEq for Path { fn eq(&self, other: &Path) -> bool { self.cmp(other) == Ordering::Equal } }
impl Eq for Path {}
impl PartialOrd for Path { fn partial_cmp(&self, other: &Path) -> Option<Ordering> { Some(self.cmp(other)) } }
impl Ord for Path {
    fn cmp(&self, other: &Path) -> Ordering {
        let left = self.inner.as_encoded_bytes(); let right = other.inner.as_encoded_bytes();
        let mut a = 0; let mut b = 0;
        loop {
            match (component(left, &mut a), component(right, &mut b)) {
                (None, None) => return Ordering::Equal,
                (None, _) => return Ordering::Less, (_, None) => return Ordering::Greater,
                (Some((ka, sa, ea)), Some((kb, sb, eb))) => {
                    let order = ka.cmp(&kb).then_with(|| left[sa..ea].cmp(&right[sb..eb]));
                    if order != Ordering::Equal { return order; }
                }
            }
        }
    }
}
impl Hash for Path {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Rust's Unix Path hashing combines normalized byte chunks and rotated
        // lengths. Root separators themselves do not contribute payload bytes.
        let bytes = self.inner.as_encoded_bytes();
        let mut start = 0;
        let mut chunk_bits = 0usize;
        for index in 0..bytes.len() {
            if bytes[index] == b'/' {
                if index > start {
                    chunk_bits = chunk_bits.wrapping_add(index - start).rotate_right(2);
                    state.write(&bytes[start..index]);
                }
                start = index + 1;
                if start < bytes.len() && bytes[start] == b'.' && (start + 1 == bytes.len() || bytes[start + 1] == b'/') { start += 1; }
            }
        }
        if start < bytes.len() {
            chunk_bits = chunk_bits.wrapping_add(bytes.len() - start).rotate_right(2);
            state.write(&bytes[start..]);
        }
        state.write_usize(chunk_bits);
    }
}
impl PartialEq for PathBuf { fn eq(&self, other: &Self) -> bool { self.as_path() == other.as_path() } }
impl Eq for PathBuf {}
impl PartialOrd for PathBuf { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
impl Ord for PathBuf { fn cmp(&self, other: &Self) -> Ordering { self.as_path().cmp(other.as_path()) } }
impl Hash for PathBuf { fn hash<H: Hasher>(&self, state: &mut H) { self.as_path().hash(state); } }
impl core::fmt::Debug for Path {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { self.inner.fmt(f) }
}
impl core::fmt::Debug for PathBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { self.as_path().fmt(f) }
}
pub struct Display<'a> { path: &'a Path }
impl core::fmt::Display for Display<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { core::fmt::Display::fmt(&self.path.to_string_lossy(), f) }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripPrefixError(());
impl core::fmt::Display for StripPrefixError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.write_str("prefix not found") }
}
impl core::error::Error for StripPrefixError {}
