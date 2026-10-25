//! Environment queries for UTF-8 variable names on Unix targets.
use crate::alloc_crate::string::String;
use crate::alloc_crate::vec::Vec;
use crate::ffi::OsString;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VarError { NotPresent, NotUnicode(OsString) }
impl core::fmt::Display for VarError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VarError::NotPresent => f.write_str("environment variable not found"),
            VarError::NotUnicode(value) => core::write!(f, "environment variable was not valid unicode: {:?}", value),
        }
    }
}
impl core::error::Error for VarError {}

unsafe extern "C" { fn getenv(name: *const u8) -> *mut u8; }

/// Reads a variable, retaining non-Unicode values in VarError::NotUnicode.
/// Other forms of OS-string keys and environment mutation are not exposed.
pub fn var(key: &str) -> Result<String, VarError> {
    if key.as_bytes().contains(&0) || key.as_bytes().contains(&b'=') { return Err(VarError::NotPresent); }
    let mut name = key.as_bytes().to_vec();
    name.push(0);
    // SAFETY: name is NUL-terminated with no interior NUL. Any non-null
    // result points to the process environment's NUL-terminated value. Copy
    // it before returning; environment mutation is not exposed by this API.
    let bytes = unsafe {
        let value = getenv(name.as_ptr());
        if value.is_null() { return Err(VarError::NotPresent); }
        let mut bytes = Vec::new();
        let mut index = 0;
        while *value.add(index) != 0 { bytes.push(*value.add(index)); index += 1; }
        bytes
    };
    String::from_utf8(bytes).map_err(|error| VarError::NotUnicode(OsString { bytes: error.into_bytes() }))
}

pub mod consts {
    #[cfg(target_os = "macos")]
    pub const OS: &str = "macos";
    #[cfg(target_os = "linux")]
    pub const OS: &str = "linux";
    pub const FAMILY: &str = "unix";
    #[cfg(target_arch = "aarch64")]
    pub const ARCH: &str = "aarch64";
    #[cfg(target_arch = "x86_64")]
    pub const ARCH: &str = "x86_64";
}
