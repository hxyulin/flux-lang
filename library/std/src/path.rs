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
    /// Returns the final normal component, ignoring trailing separators and `.`.
    pub fn file_name(&self) -> Option<&OsStr> {
        let bytes = self.inner.as_encoded_bytes();
        let mut position = 0;
        let mut last = None;
        while let Some((kind, start, end)) = component(bytes, &mut position) {
            last = if kind == 3 { Some((start, end)) } else { None };
        }
        last.map(|(start, end)| OsStr::from_unix_bytes(&bytes[start..end]))
    }
    /// Returns the filename before its final extension, preserving Unix bytes.
    pub fn file_stem(&self) -> Option<&OsStr> {
        self.file_name().map(|name| split_file_name(name).0)
    }
    /// Returns the final extension, excluding its dot. Leading dots do not split.
    pub fn extension(&self) -> Option<&OsStr> {
        self.file_name().and_then(|name| split_file_name(name).1)
    }
    pub fn components(&self) -> Components<'_> {
        Components { path: self, front: 0, back: self.as_os_str().len() }
    }
    pub fn ancestors(&self) -> Ancestors<'_> { Ancestors { next: Some(self) } }
    /// Returns false for any metadata lookup error.
    pub fn exists(&self) -> bool { crate::fs::metadata(self).is_ok() }
    /// Distinguishes missing paths from other metadata lookup failures.
    pub fn try_exists(&self) -> crate::io::Result<bool> {
        match crate::fs::metadata(self) {
            Ok(_) => Ok(true),
            Err(error) => if error.kind() == crate::io::ErrorKind::NotFound { Ok(false) } else { Err(error) },
        }
    }
    pub fn with_file_name<S: AsRef<OsStr>>(&self, file_name: S) -> PathBuf {
        let mut result = match self.file_name() {
            Some(_) => self.parent().unwrap().to_path_buf(),
            None => self.to_path_buf(),
        };
        result.push(Path::new(file_name.as_ref()));
        result
    }
    /// Replaces the final extension, preserving all preceding path bytes.
    ///
    /// # Panics
    /// Panics if the extension contains a path separator.
    pub fn with_extension<S: AsRef<OsStr>>(&self, extension: S) -> PathBuf {
        let extension = extension.as_ref();
        let bytes = self.inner.as_encoded_bytes();
        // Copy through the old extension's dot, then use the same mutation
        // operation as PathBuf. This also preserves Rust's component handling
        // when the shortened filename becomes a parent component.
        let mut end = bytes.len();
        if let Some(name) = self.file_name() {
            let name = name.as_encoded_bytes();
            let mut at = name.len();
            while at > 0 {
                at -= 1;
                if name[at] == b'.' {
                    if at != 0 { end -= name.len() - at - 1; }
                    break;
                }
            }
        }
        let mut result = PathBuf { inner: OsString { bytes: bytes[..end].to_vec() } };
        result.set_extension(extension);
        result
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
    /// Transfers the owned path bytes into a boxed path view.
    pub fn into_boxed_path(self) -> crate::alloc_crate::boxed::Box<Path> {
        use crate::alloc_crate::boxed::Box;
        let os = self.inner.into_boxed_os_str();
        // SAFETY: transparent Path has exactly OsStr's layout and validity.
        // The cast preserves length metadata and transfers allocation ownership.
        unsafe { Box::from_raw(Box::into_raw(os) as *mut Path) }
    }

    pub fn new() -> Self { Self::default() }
    pub fn as_path(&self) -> &Path { Path::new(&self.inner) }
    pub fn into_os_string(self) -> OsString { self.inner }
    /// Changes the final filename's extension, returning false without a filename.
    ///
    /// # Panics
    /// Panics if the extension contains a path separator.
    pub fn set_extension<S: AsRef<OsStr>>(&mut self, extension: S) -> bool {
        let extension = extension.as_ref().as_encoded_bytes();
        assert!(!extension.contains(&b'/'), "extension cannot contain path separators");
        let Some(name) = self.file_name() else { return false; };
        let name = name.as_encoded_bytes();
        let mut stem_len = name.len();
        let mut at = name.len();
        while at > 0 {
            at -= 1;
            if name[at] == b'.' {
                if at != 0 { stem_len = at; }
                break;
            }
        }
        let bytes = self.inner.as_encoded_bytes();
        let mut position = 0;
        let mut name_start = 0;
        while let Some((_, start, _)) = component(bytes, &mut position) { name_start = start; }
        self.inner.bytes.truncate(name_start + stem_len);
        if !extension.is_empty() { self.inner.bytes.push(b'.'); self.inner.bytes.extend_from_slice(extension); }
        true
    }
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
impl Clone for crate::alloc_crate::boxed::Box<Path> {
    fn clone(&self) -> Self { self.to_path_buf().into_boxed_path() }
}
impl PartialEq<Path> for PathBuf { fn eq(&self, other: &Path) -> bool { self.as_path() == other } }
impl PartialEq<PathBuf> for Path { fn eq(&self, other: &PathBuf) -> bool { self == other.as_path() } }
impl PartialEq<&Path> for PathBuf { fn eq(&self, other: &&Path) -> bool { self.as_path() == *other } }
impl PartialEq<PathBuf> for &Path { fn eq(&self, other: &PathBuf) -> bool { *self == other.as_path() } }
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

fn split_file_name(name: &OsStr) -> (&OsStr, Option<&OsStr>) {
    let bytes = name.as_encoded_bytes();
    match bytes.iter().rposition(|&byte| byte == b'.') {
        Some(dot) if dot != 0 => (OsStr::from_unix_bytes(&bytes[..dot]), Some(OsStr::from_unix_bytes(&bytes[dot + 1..]))),
        _ => (name, None),
    }
}

/// A Windows prefix cannot occur in Unix paths; this view keeps the public
/// component type usable in platform-independent pattern matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrefixComponent<'a> { raw: &'a OsStr }
impl<'a> PrefixComponent<'a> { pub fn as_os_str(&self) -> &'a OsStr { self.raw } }

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Component<'a> {
    Prefix(PrefixComponent<'a>),
    RootDir,
    CurDir,
    ParentDir,
    Normal(&'a OsStr),
}
impl<'a> Component<'a> {
    pub fn as_os_str(&self) -> &'a OsStr {
        match self {
            Component::Prefix(prefix) => prefix.as_os_str(),
            Component::RootDir => OsStr::new("/"),
            Component::CurDir => OsStr::new("."),
            Component::ParentDir => OsStr::new(".."),
            Component::Normal(value) => value,
        }
    }
}
impl AsRef<OsStr> for Component<'_> { fn as_ref(&self) -> &OsStr { self.as_os_str() } }
impl AsRef<Path> for Component<'_> { fn as_ref(&self) -> &Path { Path::new(self.as_os_str()) } }

/// A borrowed double-ended iterator over normalized Unix path components.
#[derive(Clone)]
pub struct Components<'a> { path: &'a Path, front: usize, back: usize }
impl<'a> Components<'a> {
    pub fn as_path(&self) -> &'a Path {
        let bytes = self.path.as_os_str().as_encoded_bytes();
        let mut position = self.front;
        let mut start = self.back;
        let mut end = self.back;
        let mut first = true;
        while let Some((_, left, right)) = component(&bytes[..self.back], &mut position) {
            if first { start = left; first = false; }
            end = right;
        }
        Path::new(OsStr::from_unix_bytes(&bytes[start..end]))
    }
    fn value(&self, kind: u8, start: usize, end: usize) -> Component<'a> {
        match kind {
            0 => Component::RootDir,
            1 => Component::CurDir,
            2 => Component::ParentDir,
            _ => Component::Normal(OsStr::from_unix_bytes(&self.path.as_os_str().as_encoded_bytes()[start..end])),
        }
    }
}
impl<'a> Iterator for Components<'a> {
    type Item = Component<'a>;
    fn next(&mut self) -> Option<Component<'a>> {
        let bytes = self.path.as_os_str().as_encoded_bytes();
        let (kind, start, end) = component(&bytes[..self.back], &mut self.front)?;
        Some(self.value(kind, start, end))
    }
}
impl<'a> DoubleEndedIterator for Components<'a> {
    fn next_back(&mut self) -> Option<Component<'a>> {
        let bytes = self.path.as_os_str().as_encoded_bytes();
        let mut position = self.front;
        let mut last = None;
        while let Some(value) = component(&bytes[..self.back], &mut position) { last = Some(value); }
        let (kind, start, end) = last?;
        self.back = start;
        Some(self.value(kind, start, end))
    }
}
impl core::iter::FusedIterator for Components<'_> {}
impl AsRef<Path> for Components<'_> { fn as_ref(&self) -> &Path { self.as_path() } }
impl AsRef<OsStr> for Components<'_> { fn as_ref(&self) -> &OsStr { self.as_path().as_os_str() } }
impl core::fmt::Debug for Components<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let components: crate::alloc_crate::vec::Vec<_> = self.clone().collect();
        f.debug_tuple("Components").field(&components).finish()
    }
}
impl PartialEq for Components<'_> { fn eq(&self, other: &Self) -> bool { self.clone().eq(other.clone()) } }
impl Eq for Components<'_> {}
impl PartialOrd for Components<'_> { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
impl Ord for Components<'_> { fn cmp(&self, other: &Self) -> Ordering { self.clone().cmp(other.clone()) } }

#[derive(Clone, Debug)]
pub struct Ancestors<'a> { next: Option<&'a Path> }
impl<'a> Iterator for Ancestors<'a> {
    type Item = &'a Path;
    fn next(&mut self) -> Option<&'a Path> {
        let next = self.next?;
        self.next = next.parent();
        Some(next)
    }
}
impl core::iter::FusedIterator for Ancestors<'_> {}
