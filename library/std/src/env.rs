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
    pub const DLL_PREFIX: &str = "lib";
    #[cfg(target_os = "macos")]
    pub const DLL_SUFFIX: &str = ".dylib";
    #[cfg(target_os = "linux")]
    pub const DLL_SUFFIX: &str = ".so";
    pub const EXE_SUFFIX: &str = "";
    pub const EXE_EXTENSION: &str = "";
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
/// Copies the process arguments without requiring Unicode.
pub fn args_os() -> ArgsOs {
    #[cfg(target_os = "macos")]
    let (count, values) = {
        unsafe extern "C" {
            fn _NSGetArgc() -> *mut i32;
            fn _NSGetArgv() -> *mut *mut *const u8;
        }
        // SAFETY: Darwin owns argc and argv for the process lifetime.
        unsafe { (*_NSGetArgc() as usize, *_NSGetArgv()) }
    };
    #[cfg(target_os = "linux")]
    let (count, values) = core::runtime::args();
    let mut arguments = Vec::new();
    for index in 0..count {
        // SAFETY: the native runtime supplies count immutable C strings.
        // The iterator owns copies and retains no references into argv.
        unsafe { arguments.push(OsString { bytes: copy_environment_string(*values.add(index)) }); }
    }
    ArgsOs { inner: arguments.into_iter(), marker: core::marker::PhantomData }
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

/// The process environment as Unicode strings, copied when created.
pub struct Vars { inner: VarsOs }
impl Iterator for Vars {
    type Item = (String, String);
    /// Panics if a name or value is not valid Unicode.
    fn next(&mut self) -> Option<(String, String)> {
        self.inner.next().map(|(a, b)| (a.into_string().unwrap(), b.into_string().unwrap()))
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl core::fmt::Debug for Vars {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Vars").field("inner", &self.inner).finish()
    }
}
/// Copies the environment; iteration panics on non-Unicode entries.
pub fn vars() -> Vars { Vars { inner: vars_os() } }

/// The process arguments as Unicode strings.
pub struct Args { inner: ArgsOs }
impl Iterator for Args {
    type Item = String;
    /// Panics if an argument is not valid Unicode.
    fn next(&mut self) -> Option<String> { self.inner.next().map(|s| s.into_string().unwrap()) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.inner.size_hint() }
}
impl ExactSizeIterator for Args { fn len(&self) -> usize { self.inner.len() } }
impl DoubleEndedIterator for Args {
    fn next_back(&mut self) -> Option<String> { self.inner.next_back().map(|s| s.into_string().unwrap()) }
}
impl core::fmt::Debug for Args {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Args").field("inner", &self.inner).finish()
    }
}
/// Copies the arguments; iteration panics on non-Unicode arguments.
pub fn args() -> Args { Args { inner: args_os() } }

/// Returns HOME when it is set and not empty, and otherwise the user
/// database's home directory for the current user.
pub fn home_dir() -> Option<crate::path::PathBuf> {
    match var_os("HOME") {
        Some(home) if !home.is_empty() => Some(crate::path::PathBuf::from(home)),
        _ => password_home().map(crate::path::PathBuf::from),
    }
}

fn password_home() -> Option<OsString> {
    unsafe extern "C" {
        fn sysconf(name: i32) -> core::ffi::c_long;
        fn getuid() -> u32;
        fn getpwuid_r(uid: u32, record: *mut u64, buffer: *mut u8, size: usize, result: *mut *mut u64) -> i32;
    }
    // _SC_GETPW_R_SIZE_MAX, and the word of struct passwd holding pw_dir.
    #[cfg(target_os = "macos")]
    const SIZE_MAX_NAME: i32 = 71;
    #[cfg(target_os = "macos")]
    const DIRECTORY_WORD: usize = 6;
    #[cfg(target_os = "linux")]
    const SIZE_MAX_NAME: i32 = 70;
    #[cfg(target_os = "linux")]
    const DIRECTORY_WORD: usize = 4;
    // SAFETY: sysconf takes no pointers.
    let size = match unsafe { sysconf(SIZE_MAX_NAME) } { n if n < 0 => 512, n => n as usize };
    let mut buffer: Vec<u8> = Vec::with_capacity(size);
    // Large enough for struct passwd on both targets (72 and 48 bytes).
    let mut record = [0u64; 10];
    let mut result: *mut u64 = core::ptr::null_mut();
    // SAFETY: record and buffer are writable for the sizes passed; on success
    // result points at record, whose directory field names a terminated
    // string inside buffer, copied before buffer is freed.
    unsafe {
        if getpwuid_r(getuid(), record.as_mut_ptr(), buffer.as_mut_ptr(), buffer.capacity(), &mut result) != 0 || result.is_null() {
            return None;
        }
        Some(OsString { bytes: copy_environment_string(*result.add(DIRECTORY_WORD) as *const u8) })
    }
}

/// An iterator over the `:`-separated parts of a PATH-style value.
pub struct SplitPaths<'a> { rest: Option<&'a [u8]> }
/// Splits a PATH-style value at each `:`; empty parts yield empty paths.
pub fn split_paths<T: AsRef<crate::ffi::OsStr> + ?Sized>(unparsed: &T) -> SplitPaths<'_> {
    SplitPaths { rest: Some(unparsed.as_ref().as_encoded_bytes()) }
}
impl<'a> Iterator for SplitPaths<'a> {
    type Item = crate::path::PathBuf;
    fn next(&mut self) -> Option<crate::path::PathBuf> {
        let rest = self.rest?;
        let part = match rest.iter().position(|&byte| byte == b':') {
            Some(index) => { self.rest = Some(&rest[index + 1..]); &rest[..index] }
            None => { self.rest = None; rest }
        };
        Some(crate::path::PathBuf::from(OsString { bytes: part.to_vec() }))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.rest { Some(rest) => (1, Some(rest.len() + 1)), None => (0, Some(0)) }
    }
}
impl core::fmt::Debug for SplitPaths<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SplitPaths").finish_non_exhaustive()
    }
}

/// The error from join_paths when a part contains the `:` separator.
#[derive(Debug)]
pub struct JoinPathsError { inner: paths::JoinPathsError }
mod paths {
    #[derive(Debug)]
    pub struct JoinPathsError;
}
impl core::fmt::Display for JoinPathsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let paths::JoinPathsError = self.inner;
        f.write_str("path segment contains separator `:`")
    }
}
impl core::error::Error for JoinPathsError {}
/// Joins parts with `:`, failing if a part contains `:`.
pub fn join_paths<I, T>(paths: I) -> Result<OsString, JoinPathsError>
where
    I: IntoIterator<Item = T>,
    T: AsRef<crate::ffi::OsStr>,
{
    let mut joined = Vec::new();
    for (index, path) in paths.into_iter().enumerate() {
        let path = path.as_ref().as_encoded_bytes();
        if index > 0 { joined.push(b':'); }
        if path.contains(&b':') { return Err(JoinPathsError { inner: paths::JoinPathsError }); }
        joined.extend_from_slice(path);
    }
    Ok(OsString { bytes: joined })
}
