//! C interoperability and owned operating-system strings.
pub use core::ffi::*;
pub use crate::alloc_crate::ffi::{CString, NulError, IntoStringError, FromVecWithNulError};
mod os_str;
pub use self::os_str::OsStr;
use crate::alloc_crate::borrow::Cow;
use crate::alloc_crate::string::String;
use crate::alloc_crate::vec::Vec;

/// Owned Unix operating-system bytes.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OsString { pub(crate) bytes: Vec<u8> }
impl OsString {
    /// Transfers the owned bytes into a boxed OS string view.
    pub fn into_boxed_os_str(self) -> crate::alloc_crate::boxed::Box<OsStr> {
        use crate::alloc_crate::boxed::Box;
        let bytes = self.bytes.into_boxed_slice();
        // SAFETY: transparent OsStr has the same layout and metadata as [u8].
        // Every Unix byte sequence is valid, and ownership transfers to the box.
        unsafe { Box::from_raw(Box::into_raw(bytes) as *mut OsStr) }
    }
    pub fn new() -> Self { Self { bytes: Vec::new() } }
    pub fn with_capacity(capacity: usize) -> Self { Self { bytes: Vec::with_capacity(capacity) } }
    pub fn capacity(&self) -> usize { self.bytes.capacity() }
    pub fn clear(&mut self) { self.bytes.clear() }
    pub fn reserve(&mut self, additional: usize) { self.bytes.reserve(additional) }
    pub fn reserve_exact(&mut self, additional: usize) { self.bytes.reserve_exact(additional) }
    pub fn shrink_to_fit(&mut self) { self.bytes.shrink_to_fit() }
    /// Appends the bytes of `value`.
    pub fn push<T: AsRef<OsStr>>(&mut self, value: T) { self.bytes.extend_from_slice(value.as_ref().as_encoded_bytes()) }
    pub fn into_encoded_bytes(self) -> Vec<u8> { self.bytes }
    /// # Safety
    /// On Unix every byte sequence is a valid OS string.
    pub unsafe fn from_encoded_bytes_unchecked(bytes: Vec<u8>) -> Self { Self { bytes } }
    pub fn to_str(&self) -> Option<&str> { core::str::from_utf8(&self.bytes).ok() }
    pub fn to_string_lossy(&self) -> Cow<'_, str> { String::from_utf8_lossy(&self.bytes) }
    pub fn as_encoded_bytes(&self) -> &[u8] { &self.bytes }
    pub fn into_string(self) -> Result<String, Self> {
        match String::from_utf8(self.bytes) {
            Ok(value) => Ok(value),
            Err(error) => Err(Self { bytes: error.into_bytes() }),
        }
    }
}
impl Default for OsString { fn default() -> Self { Self::new() } }
impl From<String> for OsString { fn from(value: String) -> Self { Self { bytes: value.into_bytes() } } }
impl<T: ?Sized + AsRef<OsStr>> From<&T> for OsString { fn from(value: &T) -> Self { Self { bytes: value.as_ref().as_encoded_bytes().to_vec() } } }
impl core::fmt::Debug for OsString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("\"")?;
        let mut rest = self.bytes.as_slice();
        while !rest.is_empty() {
            let (valid, invalid) = match core::str::from_utf8(rest) {
                Ok(_) => (rest.len(), 0),
                Err(error) => (error.valid_up_to(), error.error_len().unwrap_or(rest.len() - error.valid_up_to())),
            };
            if valid != 0 {
                let text = core::str::from_utf8(&rest[..valid]).unwrap();
                let escaped = crate::alloc_crate::format!("{:?}", text);
                f.write_str(&escaped[1..escaped.len() - 1])?;
            }
            for &byte in &rest[valid..valid + invalid] {
                core::write!(f, "\\x{:02X}", byte)?;
            }
            rest = &rest[valid + invalid..];
        }
        f.write_str("\"")
    }
}
