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


/// Returns the process's current working directory, preserving Unix path bytes.
///
/// Returns an OS error if the working directory is no longer accessible.
pub fn current_dir() -> crate::io::Result<crate::path::PathBuf> {
    unsafe extern "C" { fn getcwd(buffer: *mut u8, size: usize) -> *mut u8; }
    let mut buffer = crate::alloc_crate::vec![0u8; 256];
    loop {
        // SAFETY: the writable buffer has the supplied length. On success,
        // getcwd writes a terminated path within it; on failure, errno is read
        // before making another libc call.
        if !unsafe { getcwd(buffer.as_mut_ptr(), buffer.len()) }.is_null() {
            let length = buffer.iter().position(|&byte| byte == 0).unwrap();
            buffer.truncate(length);
            return Ok(crate::path::PathBuf::from(OsString { bytes: buffer }));
        }
        let error = crate::io::Error::last_os_error();
        // ERANGE has value 34 on both supported Unix targets.
        if error.raw_os_error() != Some(34) { return Err(error); }
        let length = buffer.len().checked_mul(2).expect("current directory path is too long");
        buffer.resize(length, 0);
    }
}
