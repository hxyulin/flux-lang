//! Owned NUL-terminated C strings with checked construction.
use crate::borrow::{Borrow, ToOwned};
use crate::boxed::Box;
use crate::string::String;
use crate::vec::Vec;
use core::ffi::{CStr, c_char};
use core::hash::Hash;
use core::ops::Deref;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CString {
    inner: Box<[u8]>,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NulError(usize, Vec<u8>);
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct IntoStringError {
    inner: CString,
    error: core::str::Utf8Error,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FromVecWithNulError {
    error_kind: FromBytesWithNulErrorKind,
    bytes: Vec<u8>,
}
#[derive(Clone, PartialEq, Eq, Debug)]
enum FromBytesWithNulErrorKind {
    InteriorNul(usize),
    NotNulTerminated,
}
impl CString {
    pub fn new<T: Into<Vec<u8>>>(value: T) -> Result<Self, NulError> {
        let mut bytes = value.into();
        if let Some(position) = bytes.iter().position(|&b| b == 0) {
            return Err(NulError(position, bytes));
        }
        bytes.push(0);
        Ok(Self {
            inner: bytes.into_boxed_slice(),
        })
    }
    /// The input must not contain a NUL byte; this appends the final terminator.
    pub unsafe fn from_vec_unchecked(mut bytes: Vec<u8>) -> Self {
        bytes.push(0);
        Self {
            inner: bytes.into_boxed_slice(),
        }
    }
    pub fn from_vec_with_nul(bytes: Vec<u8>) -> Result<Self, FromVecWithNulError> {
        match bytes.iter().position(|&b| b == 0) {
            Some(position) if position + 1 == bytes.len() => Ok(Self {
                inner: bytes.into_boxed_slice(),
            }),
            Some(position) => Err(FromVecWithNulError {
                bytes,
                error_kind: FromBytesWithNulErrorKind::InteriorNul(position),
            }),
            None => Err(FromVecWithNulError {
                bytes,
                error_kind: FromBytesWithNulErrorKind::NotNulTerminated,
            }),
        }
    }
    /// The input must contain exactly one NUL, at its final byte.
    pub unsafe fn from_vec_with_nul_unchecked(bytes: Vec<u8>) -> Self {
        Self {
            inner: bytes.into_boxed_slice(),
        }
    }
    pub fn as_c_str(&self) -> &CStr {
        // SAFETY: constructors establish a sole trailing NUL. CStr carries
        // the same length metadata as its backing byte slice, including NUL.
        unsafe { &*(&*self.inner as *const [u8] as *const CStr) }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner[..self.inner.len() - 1]
    }
    pub fn as_bytes_with_nul(&self) -> &[u8] {
        &self.inner
    }
    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = self.inner.into_vec();
        bytes.pop();
        bytes
    }
    pub fn into_bytes_with_nul(self) -> Vec<u8> {
        self.inner.into_vec()
    }
    pub fn into_string(self) -> Result<String, IntoStringError> {
        match String::from_utf8(self.into_bytes()) {
            Ok(value) => Ok(value),
            Err(error) => {
                let utf8_error = error.utf8_error();
                // SAFETY: removing the final terminator retained the no-NUL
                // invariant even when the preceding bytes were invalid UTF-8.
                let inner = unsafe { Self::from_vec_unchecked(error.into_bytes()) };
                Err(IntoStringError {
                    inner,
                    error: utf8_error,
                })
            }
        }
    }
    pub fn into_boxed_c_str(self) -> Box<CStr> {
        // SAFETY: byte and CStr tails share layout and length metadata. The
        // allocation retains its single owner and its trailing NUL invariant.
        unsafe { Box::from_raw(Box::into_raw(self.inner) as *mut CStr) }
    }
    pub fn into_raw(self) -> *mut c_char {
        Box::into_raw(self.inner) as *mut u8 as *mut c_char
    }
    /// Reclaims a pointer returned by into_raw. Its allocation and terminator
    /// position must be unchanged and no other owner may reclaim it.
    pub unsafe fn from_raw(pointer: *mut c_char) -> Self {
        // SAFETY: the caller guarantees the original live CString allocation.
        let len = unsafe { CStr::from_ptr(pointer) }.to_bytes_with_nul().len();
        let pointer = core::ptr::slice_from_raw_parts_mut(pointer as *mut u8, len);
        // SAFETY: the scan recovered the original boxed-slice metadata.
        Self {
            inner: unsafe { Box::from_raw(pointer) },
        }
    }
}
impl Default for CString {
    fn default() -> Self {
        Self::new(Vec::new()).unwrap()
    }
}
impl Deref for CString {
    type Target = CStr;
    fn deref(&self) -> &CStr {
        self.as_c_str()
    }
}
impl AsRef<CStr> for CString {
    fn as_ref(&self) -> &CStr {
        self.as_c_str()
    }
}
impl Borrow<CStr> for CString {
    fn borrow(&self) -> &CStr {
        self.as_c_str()
    }
}
impl core::fmt::Debug for CString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self.as_c_str(), f)
    }
}
impl core::hash::Hash for CString {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state)
    }
}
impl From<&CStr> for CString {
    fn from(value: &CStr) -> Self {
        Self {
            inner: value.to_bytes_with_nul().into(),
        }
    }
}
impl From<CString> for Box<CStr> {
    fn from(value: CString) -> Self {
        value.into_boxed_c_str()
    }
}
impl From<Box<CStr>> for CString {
    fn from(value: Box<CStr>) -> Self {
        // SAFETY: the owned CStr tail has the same layout as its NUL-inclusive
        // byte slice. This transfers its allocation without duplicating it.
        Self {
            inner: unsafe { Box::from_raw(Box::into_raw(value) as *mut [u8]) },
        }
    }
}
impl From<&CStr> for Box<CStr> {
    fn from(value: &CStr) -> Self {
        CString::from(value).into_boxed_c_str()
    }
}
impl ToOwned for CStr {
    type Owned = CString;
    fn to_owned(&self) -> CString {
        CString::from(self)
    }
}
impl NulError {
    pub fn nul_position(&self) -> usize {
        self.0
    }
    pub fn into_vec(self) -> Vec<u8> {
        self.1
    }
}
impl core::fmt::Display for NulError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::write!(f, "nul byte found in provided data at position: {}", self.0)
    }
}
impl core::error::Error for NulError {}
impl IntoStringError {
    pub fn utf8_error(&self) -> core::str::Utf8Error {
        self.error
    }
    pub fn into_cstring(self) -> CString {
        self.inner
    }
}
impl core::fmt::Display for IntoStringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::write!(f, "C string contained non-utf8 bytes")
    }
}
impl core::error::Error for IntoStringError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        Some(&self.error)
    }
}
impl FromVecWithNulError {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
impl core::fmt::Display for FromVecWithNulError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.error_kind {
            FromBytesWithNulErrorKind::InteriorNul(position) => core::write!(
                f,
                "data provided contains an interior nul byte at pos {}",
                position
            ),
            FromBytesWithNulErrorKind::NotNulTerminated => {
                f.write_str("data provided is not nul terminated")
            }
        }
    }
}
impl core::error::Error for FromVecWithNulError {}
