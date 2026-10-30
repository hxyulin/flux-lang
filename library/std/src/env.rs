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
    match var_os(key) {
        Some(value) => value.into_string().map_err(VarError::NotUnicode),
        None => Err(VarError::NotPresent),
    }
}

/// Copies an environment value without requiring Unicode names or values.
pub fn var_os<K: AsRef<crate::ffi::OsStr>>(key: K) -> Option<OsString> {
    let key = key.as_ref().as_encoded_bytes();
    if key.contains(&0) { return None; }
    let mut name = key.to_vec();
    name.push(0);
    // SAFETY: name is terminated and contains no embedded NUL. getenv returns
    // null or a live terminated value, copied before this function returns.
    let value = unsafe { getenv(name.as_ptr()) };
    if value.is_null() { None } else { Some(OsString { bytes: {
        // SAFETY: the non-null getenv result is a readable terminated value.
        unsafe { copy_environment_string(value) }
    } }) }
}

unsafe fn copy_environment_string(pointer: *const u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut index = 0;
    // SAFETY: the caller guarantees a readable C string through its first NUL.
    unsafe { while *pointer.add(index) != 0 { bytes.push(*pointer.add(index)); index += 1; } }
    bytes
}

/// An owned snapshot of the process environment, retaining Unix bytes.
pub struct VarsOs { inner: crate::alloc_crate::vec::IntoIter<(OsString, OsString)>, marker: core::marker::PhantomData<*mut ()> }
impl Iterator for VarsOs {
    type Item = (OsString, OsString);
    fn next(&mut self) -> Option<Self::Item> { self.inner.next() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl core::fmt::Debug for VarsOs {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VarsOs").field("inner", &self.inner.as_slice()).finish()
    }
}

/// Copies the environment before exposing entries to iteration.
pub fn vars_os() -> VarsOs {
    #[cfg(target_os = "macos")]
    unsafe extern "C" { fn _NSGetEnviron() -> *mut *mut *mut u8; }
    #[cfg(target_os = "linux")]
    unsafe extern "C" { static mut environ: *mut *mut u8; }
    let mut entries = Vec::new();
    // SAFETY: the native process environment is a terminated array of readable
    // C strings. No environment mutation is exposed by this facade. Each entry
    // is copied, so the returned iterator borrows no native environment memory.
    unsafe {
        #[cfg(target_os = "macos")]
        let mut cursor = *_NSGetEnviron();
        #[cfg(target_os = "linux")]
        let mut cursor = environ;
        while !cursor.is_null() && !(*cursor).is_null() {
            let bytes = copy_environment_string(*cursor);
            if let Some(equal) = bytes.iter().enumerate().skip(1).find_map(|(index, &byte)| if byte == b'=' { Some(index) } else { None }) {
                entries.push((OsString { bytes: bytes[..equal].to_vec() }, OsString { bytes: bytes[equal + 1..].to_vec() }));
            }
            cursor = cursor.add(1);
        }
    }
    VarsOs { inner: entries.into_iter(), marker: core::marker::PhantomData }
}

/// Returns TMPDIR, or the platform's temporary directory when it is absent.
pub fn temp_dir() -> crate::path::PathBuf {
    if let Some(value) = var_os("TMPDIR") { return crate::path::PathBuf::from(value); }
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" { fn confstr(name: i32, buffer: *mut u8, length: usize) -> usize; }
        let mut buffer = crate::alloc_crate::vec![0u8; 64];
        loop {
            // SAFETY: confstr receives the Darwin per-user temp-directory key
            // and a writable buffer of exactly the supplied capacity.
            let needed = unsafe { confstr(65537, buffer.as_mut_ptr(), buffer.len()) };
            if needed == 0 { break; }
            if needed <= buffer.len() {
                buffer.truncate(needed - 1);
                return crate::path::PathBuf::from(OsString { bytes: buffer });
            }
            buffer.resize(needed, 0);
        }
    }
    crate::path::PathBuf::from("/tmp")
}

pub mod consts {
    #[cfg(target_os = "macos")]
    pub const DLL_EXTENSION: &str = "dylib";
    #[cfg(target_os = "linux")]
    pub const DLL_EXTENSION: &str = "so";
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

/// An owned snapshot of this process's command-line arguments.
/// Like Rust's ArgsOs, the iterator is neither Send nor Sync.
pub struct ArgsOs { inner: crate::alloc_crate::vec::IntoIter<OsString>, marker: core::marker::PhantomData<*mut ()> }
impl Iterator for ArgsOs {
    type Item = OsString;
    fn next(&mut self) -> Option<OsString> { self.inner.next() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl DoubleEndedIterator for ArgsOs { fn next_back(&mut self) -> Option<OsString> { self.inner.next_back() } }
impl ExactSizeIterator for ArgsOs { fn len(&self) -> usize { self.inner.len() } }
impl core::fmt::Debug for ArgsOs {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ArgsOs").field("inner", &self.inner.as_slice()).finish()
    }
}
/// Copies native Darwin argv without requiring Unicode. Linux command-line
/// access requires a runtime entrypoint handoff and remains unsupported.
pub fn args_os() -> ArgsOs {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn _NSGetArgc() -> *mut i32;
            fn _NSGetArgv() -> *mut *mut *mut u8;
        }
        let mut arguments = Vec::new();
        // SAFETY: Darwin's runtime owns argc and the argument-pointer array
        // for the process lifetime. Entries are copied through their NULs;
        // the returned iterator holds no references into runtime storage.
        unsafe {
            let count = *_NSGetArgc();
            let values = *_NSGetArgv();
            for index in 0..count as usize {
                arguments.push(OsString { bytes: copy_environment_string(*values.add(index)) });
            }
        }
        ArgsOs { inner: arguments.into_iter(), marker: core::marker::PhantomData }
    }
    #[cfg(not(target_os = "macos"))]
    { panic!("command-line argument access is not supported on this target") }
}

/// Returns the running executable's canonical path.
pub fn current_exe() -> crate::io::Result<crate::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" { fn _NSGetExecutablePath(buffer: *mut u8, size: *mut u32) -> i32; }
        let mut size = 256u32;
        loop {
            let mut buffer = crate::alloc_crate::vec![0u8; size as usize];
            // SAFETY: buffer holds size writable bytes; the runtime either
            // writes a terminated path or reports the required buffer size.
            if unsafe { _NSGetExecutablePath(buffer.as_mut_ptr(), &mut size) } == 0 {
                let length = buffer.iter().position(|&b| b == 0).unwrap();
                buffer.truncate(length);
                return crate::fs::canonicalize(crate::path::PathBuf::from(OsString { bytes: buffer }));
            }
        }
    }
    #[cfg(target_os = "linux")]
    { crate::fs::canonicalize("/proc/self/exe") }
}
