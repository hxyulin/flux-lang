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
    pub fn into_os_string(self: crate::alloc_crate::boxed::Box<OsStr>) -> OsString {
        use crate::alloc_crate::boxed::Box;
        // SAFETY: transparent OsStr has exactly [u8]'s layout and metadata.
        let bytes = unsafe { Box::from_raw(Box::into_raw(self) as *mut [u8]) };
        OsString { bytes: bytes.into_vec() }
    }
    pub fn to_ascii_lowercase(&self) -> OsString { let mut bytes = self.inner.to_vec(); bytes.make_ascii_lowercase(); OsString { bytes } }
    pub fn to_ascii_uppercase(&self) -> OsString { let mut bytes = self.inner.to_vec(); bytes.make_ascii_uppercase(); OsString { bytes } }
    pub fn make_ascii_lowercase(&mut self) { self.inner.make_ascii_lowercase() }
    pub fn make_ascii_uppercase(&mut self) { self.inner.make_ascii_uppercase() }
    pub fn is_ascii(&self) -> bool { self.inner.is_ascii() }
    pub fn eq_ignore_ascii_case<S: AsRef<OsStr>>(&self, other: S) -> bool {
        self.inner.eq_ignore_ascii_case(other.as_ref().as_encoded_bytes())
    }
    pub fn display(&self) -> Display<'_> { Display { os_str: self } }
}

/// Formats an OS string lossily, replacing invalid UTF-8 with U+FFFD.
pub struct Display<'a> { os_str: &'a OsStr }
impl core::fmt::Display for Display<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.os_str.to_string_lossy(), f)
    }
}
impl core::fmt::Debug for Display<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { core::fmt::Debug::fmt(self.os_str, f) }
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

use crate::alloc_crate::boxed::Box;
use crate::alloc_crate::rc::Rc;
use crate::alloc_crate::sync::Arc;
impl From<&OsStr> for Box<OsStr> {
    fn from(value: &OsStr) -> Box<OsStr> { value.to_os_string().into_boxed_os_str() }
}
impl From<OsString> for Box<OsStr> {
    fn from(value: OsString) -> Box<OsStr> { value.into_boxed_os_str() }
}
impl From<Box<OsStr>> for OsString {
    fn from(value: Box<OsStr>) -> OsString { value.into_os_string() }
}
impl From<Cow<'_, OsStr>> for Box<OsStr> {
    fn from(value: Cow<'_, OsStr>) -> Box<OsStr> { value.into_owned().into_boxed_os_str() }
}
impl Default for Box<OsStr> {
    fn default() -> Box<OsStr> { OsString::new().into_boxed_os_str() }
}
impl From<&OsStr> for Arc<OsStr> {
    fn from(value: &OsStr) -> Arc<OsStr> {
        let bytes: Arc<[u8]> = Arc::from(value.as_encoded_bytes());
        // SAFETY: transparent OsStr has exactly [u8]'s layout and metadata.
        unsafe { Arc::from_raw(Arc::into_raw(bytes) as *const OsStr) }
    }
}
impl From<OsString> for Arc<OsStr> {
    fn from(value: OsString) -> Arc<OsStr> { Arc::from(value.as_os_str()) }
}
impl From<&OsStr> for Rc<OsStr> {
    fn from(value: &OsStr) -> Rc<OsStr> {
        let bytes: Rc<[u8]> = Rc::from(value.as_encoded_bytes());
        // SAFETY: transparent OsStr has exactly [u8]'s layout and metadata.
        unsafe { Rc::from_raw(Rc::into_raw(bytes) as *const OsStr) }
    }
}
impl From<OsString> for Rc<OsStr> {
    fn from(value: OsString) -> Rc<OsStr> { Rc::from(value.as_os_str()) }
}
impl<'a> From<&'a OsStr> for Cow<'a, OsStr> {
    fn from(value: &'a OsStr) -> Cow<'a, OsStr> { Cow::Borrowed(value) }
}
impl<'a> From<OsString> for Cow<'a, OsStr> {
    fn from(value: OsString) -> Cow<'a, OsStr> { Cow::Owned(value) }
}
impl<'a> From<&'a OsString> for Cow<'a, OsStr> {
    fn from(value: &'a OsString) -> Cow<'a, OsStr> { Cow::Borrowed(value.as_os_str()) }
}
impl<'a> From<Cow<'a, OsStr>> for OsString {
    fn from(value: Cow<'a, OsStr>) -> OsString { value.into_owned() }
}
impl<S: Borrow<OsStr>> crate::alloc_crate::slice::Join<&OsStr> for [S] {
    type Output = OsString;
    fn join(slice: &Self, separator: &OsStr) -> OsString {
        let mut result = OsString::new();
        let mut first = true;
        for value in slice {
            if !first { result.push(separator); }
            first = false;
            result.push(value.borrow());
        }
        result
    }
}
