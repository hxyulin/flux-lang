//! Borrowed Unix OS strings retain arbitrary bytes and slice metadata.
use super::OsString;
use crate::alloc_crate::borrow::{Cow, ToOwned};
use crate::alloc_crate::string::String;
use core::borrow::Borrow;
use core::ops::Deref;

#[repr(transparent)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OsStr { inner: [u8] }

impl OsStr {
    /// # Safety
    /// On Unix every byte sequence is a valid encoded OS string. The caller
    /// must retain the source allocation for the returned borrow.
    pub unsafe fn from_encoded_bytes_unchecked(bytes: &[u8]) -> &OsStr { Self::from_unix_bytes(bytes) }
    pub fn new<S: AsRef<OsStr> + ?Sized>(value: &S) -> &OsStr { value.as_ref() }
    pub(crate) fn from_unix_bytes(bytes: &[u8]) -> &OsStr {
        // SAFETY: transparent OsStr has the same storage and length metadata
        // as [u8], and imposes no validity restriction on Unix bytes.
        unsafe { &*(bytes as *const [u8] as *const OsStr) }
    }
    pub fn as_encoded_bytes(&self) -> &[u8] { &self.inner }
    pub fn to_str(&self) -> Option<&str> { core::str::from_utf8(&self.inner).ok() }
    pub fn to_string_lossy(&self) -> Cow<'_, str> { String::from_utf8_lossy(&self.inner) }
    pub fn to_os_string(&self) -> OsString { OsString { bytes: self.inner.to_vec() } }
    pub fn is_empty(&self) -> bool { self.inner.is_empty() }
    pub fn len(&self) -> usize { self.inner.len() }
}
impl core::fmt::Debug for OsStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { self.to_os_string().fmt(f) }
}
impl ToOwned for OsStr {
    type Owned = OsString;
    fn to_owned(&self) -> OsString { self.to_os_string() }
}
impl AsRef<OsStr> for OsStr { fn as_ref(&self) -> &OsStr { self } }
impl AsRef<OsStr> for str { fn as_ref(&self) -> &OsStr { OsStr::from_unix_bytes(self.as_bytes()) } }
impl AsRef<OsStr> for String { fn as_ref(&self) -> &OsStr { self.as_str().as_ref() } }
impl AsRef<OsStr> for OsString { fn as_ref(&self) -> &OsStr { self.as_os_str() } }
impl Borrow<OsStr> for OsString { fn borrow(&self) -> &OsStr { self.as_os_str() } }
impl Deref for OsString {
    type Target = OsStr;
    fn deref(&self) -> &OsStr { self.as_os_str() }
}
impl OsString { pub fn as_os_str(&self) -> &OsStr { OsStr::from_unix_bytes(&self.bytes) } }
impl From<&OsStr> for OsString { fn from(value: &OsStr) -> Self { value.to_os_string() } }

impl Default for &OsStr { fn default() -> Self { OsStr::new("") } }
impl From<&String> for OsString { fn from(value: &String) -> Self { Self::from(value.as_str()) } }
impl Clone for crate::alloc_crate::boxed::Box<OsStr> {
    fn clone(&self) -> Self { self.to_os_string().into_boxed_os_str() }
}

macro_rules! os_str_eq {
    ($left:ty, $right:ty) => {
        impl PartialEq<$right> for $left {
            fn eq(&self, other: &$right) -> bool {
                let left: &OsStr = self.as_ref();
                let right: &OsStr = other.as_ref();
                left.as_encoded_bytes() == right.as_encoded_bytes()
            }
        }
    };
}
os_str_eq!(OsStr, str);
os_str_eq!(str, OsStr);
os_str_eq!(OsString, str);
os_str_eq!(str, OsString);
os_str_eq!(OsString, &str);
os_str_eq!(&str, OsString);
os_str_eq!(OsString, OsStr);
os_str_eq!(OsStr, OsString);
os_str_eq!(OsString, &OsStr);
os_str_eq!(&OsStr, OsString);
