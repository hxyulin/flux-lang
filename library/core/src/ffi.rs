//! Types for calling C: its integer types on flux's targets (64-bit
//! Apple arm64 and Linux), and `CStr`.

use crate::clone::Clone;
use crate::cmp::Ordering;
use crate::cmp::{Eq, Ord, PartialEq, PartialOrd};
use crate::default::Default;
use crate::fmt::{self, Debug, Formatter};
use crate::intrinsics::{c_str_from_ptr, slice_from_raw_parts};
use crate::marker::Copy;
use crate::option::Option::{self, None, Some};
use crate::result::Result::{self, Err, Ok};

/// A borrowed C string, NUL-terminated: what `c"..."` gives, as
/// `&'static CStr`. The compiler knows its layout: a `&CStr` is a
/// pointer to its first byte, where rustc's also has its length, so
/// its length is counted where it is needed. `as_ptr` is built in.
#[lang = "CStr"]
pub struct CStr {
    _unsized: (),
}

/// Why [`CStr::from_bytes_with_nul`] failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FromBytesWithNulError {
    /// A NUL before the last byte, at byte `position`.
    InteriorNul { position: usize },
    /// The last byte is not NUL.
    NotNulTerminated,
}

/// Why [`CStr::from_bytes_until_nul`] failed: there is no NUL.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FromBytesUntilNulError(());

impl CStr {
    /// The C string `bytes` is, which must end in its only NUL.
    pub fn from_bytes_with_nul(bytes: &[u8]) -> Result<&CStr, FromBytesWithNulError> {
        match nul_position(bytes) {
            Some(position) if position + 1 == bytes.len() => {
                Ok(unsafe { c_str_from_ptr(bytes.as_ptr()) })
            }
            Some(position) => Err(FromBytesWithNulError::InteriorNul { position }),
            None => Err(FromBytesWithNulError::NotNulTerminated),
        }
    }

    /// The C string at the start of `bytes`, up to its first NUL.
    pub fn from_bytes_until_nul(bytes: &[u8]) -> Result<&CStr, FromBytesUntilNulError> {
        match nul_position(bytes) {
            Some(_) => Ok(unsafe { c_str_from_ptr(bytes.as_ptr()) }),
            None => Err(FromBytesUntilNulError(())),
        }
    }

    /// Its length in bytes, without the NUL.
    pub fn count_bytes(&self) -> usize {
        let start = self.as_ptr() as usize;
        let mut len = 0;
        while unsafe { *((start + len) as *const u8) } != 0 {
            len += 1;
        }
        len
    }

    /// Whether it has no bytes before the NUL.
    pub fn is_empty(&self) -> bool {
        unsafe { *(self.as_ptr() as *const u8) == 0 }
    }

    /// Its bytes, without the NUL.
    pub fn to_bytes(&self) -> &[u8] {
        unsafe { slice_from_raw_parts(self.as_ptr() as *const u8, self.count_bytes()) }
    }

    /// Its bytes, with the NUL.
    pub fn to_bytes_with_nul(&self) -> &[u8] {
        unsafe { slice_from_raw_parts(self.as_ptr() as *const u8, self.count_bytes() + 1) }
    }

    /// Its bytes as a `str`, if they are UTF-8.
    pub fn to_str(&self) -> Result<&str, crate::str::Utf8Error> {
        crate::str::from_utf8(self.to_bytes())
    }
}

/// Where the first NUL in `bytes` is, if there is one.
fn nul_position(bytes: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Byte by byte, as rustc's derived impl compares their bytes.
impl PartialEq for CStr {
    fn eq(&self, other: &CStr) -> bool {
        self.to_bytes() == other.to_bytes()
    }
}

impl Eq for CStr {}

impl PartialEq<&CStr> for CStr {
    fn eq(&self, other: &&CStr) -> bool {
        PartialEq::eq(self, *other)
    }
}

impl PartialOrd for CStr {
    fn partial_cmp(&self, other: &CStr) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Byte by byte, as rustc's compares their bytes.
impl Ord for CStr {
    fn cmp(&self, other: &CStr) -> Ordering {
        let (a, b) = (self.to_bytes(), other.to_bytes());
        let mut i = 0;
        while i < a.len() && i < b.len() {
            if a[i] != b[i] {
                return if a[i] < b[i] { Ordering::Less } else { Ordering::Greater };
            }
            i += 1;
        }
        a.len().cmp(&b.len())
    }
}

impl Default for &CStr {
    fn default() -> Self {
        c""
    }
}

/// Its bytes in quotes, as rustc's (a `ByteStr`'s): ASCII escaped as
/// `u8::escape_ascii` does, other characters as `char::escape_debug`,
/// and bytes that are not UTF-8 as `\xNN`.
impl Debug for CStr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("\"")?;
        let bytes = self.to_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b < 0x80 {
                fmt::escape_ascii(b, f)?;
                i += 1;
                continue;
            }
            match utf8_char(bytes, i) {
                Some((c, len)) => {
                    match fmt::escape_debug(c, true, true) {
                        Some(escape) => escape.write(f)?,
                        None => fmt::Write::write_char(f, c)?,
                    }
                    i += len;
                }
                None => {
                    fmt::escape_ascii(b, f)?;
                    i += 1;
                }
            }
        }
        f.write_str("\"")
    }
}

/// The `char` whose UTF-8 bytes start at `bytes[i]`, and how many there
/// are, if they are UTF-8.
fn utf8_char(bytes: &[u8], i: usize) -> Option<(char, usize)> {
    let b = bytes[i] as u32;
    let (len, init, min) = if b >= 0xC2 && b < 0xE0 {
        (2, b & 0x1F, 0x80)
    } else if b >= 0xE0 && b < 0xF0 {
        (3, b & 0x0F, 0x800)
    } else if b >= 0xF0 && b < 0xF5 {
        (4, b & 0x07, 0x10000)
    } else {
        return None;
    };
    if i + len > bytes.len() {
        return None;
    }
    let mut c = init;
    let mut k = 1;
    while k < len {
        let next = bytes[i + k] as u32;
        if next & 0xC0 != 0x80 {
            return None;
        }
        c = (c << 6) | (next & 0x3F);
        k += 1;
    }
    if c < min || c > 0x10FFFF || (c >= 0xD800 && c <= 0xDFFF) {
        return None;
    }
    Some((unsafe { crate::char::from_u32_unchecked(c) }, len))
}

pub type c_char = i8;
pub type c_schar = i8;
pub type c_uchar = u8;
pub type c_short = i16;
pub type c_ushort = u16;
pub type c_int = i32;
pub type c_uint = u32;
pub type c_long = i64;
pub type c_ulong = u64;
pub type c_longlong = i64;
pub type c_ulonglong = u64;
