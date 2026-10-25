//! The filesystem error surface used by source loading.
use crate::alloc_crate::string::String;

pub type Result<T> = core::result::Result<T, Error>;
pub struct Error { code: i32, message: Option<&'static str> }
#[cfg(target_os = "macos")]
unsafe extern "C" { fn __error() -> *mut i32; }
#[cfg(target_os = "linux")]
unsafe extern "C" { fn __errno_location() -> *mut i32; }
#[cfg(target_os = "macos")]
unsafe extern "C" { fn strerror_r(code: i32, buffer: *mut core::ffi::c_char, length: usize) -> i32; }
#[cfg(target_os = "linux")]
unsafe extern "C" {
    #[link_name = "__xpg_strerror_r"]
    fn strerror_r(code: i32, buffer: *mut core::ffi::c_char, length: usize) -> i32;
}

impl Error {
    pub fn last_os_error() -> Error {
        // SAFETY: libc returns this thread's live errno slot.
        let code = unsafe {
            #[cfg(target_os = "macos")]
            { *__error() }
            #[cfg(target_os = "linux")]
            { *__errno_location() }
        };
        Error { code, message: None }
    }
    pub fn from_raw_os_error(code: i32) -> Error { Error { code, message: None } }
    pub fn raw_os_error(&self) -> Option<i32> { if self.message.is_none() { Some(self.code) } else { None } }
    pub(crate) fn nul_path() -> Error { Error { code: 0, message: Some("file name contained an unexpected NUL byte") } }
    pub(crate) fn invalid_utf8() -> Error { Error { code: 0, message: Some("stream did not contain valid UTF-8") } }
    fn description(&self) -> String {
        let mut buffer = [0u8; 1024];
        // SAFETY: libc writes at most this local buffer's supplied capacity.
        // Reentrant strerror_r avoids shared buffers during parallel errors.
        unsafe { strerror_r(self.code, buffer.as_mut_ptr() as *mut core::ffi::c_char, buffer.len()); }
        let end = buffer.iter().position(|&byte| byte == 0).unwrap_or(buffer.len());
        String::from_utf8_lossy(&buffer[..end]).into_owned()
    }
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.message {
            Some(message) => f.write_str(message),
            None => core::write!(f, "{} (os error {})", self.description(), self.code),
        }
    }
}
impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(message) = self.message {
            let kind = if self.code == 0 && message == "stream did not contain valid UTF-8" { "InvalidData" } else { "InvalidInput" };
            core::write!(f, "Error {{ kind: {}, message: {:?} }}", kind, message)
        } else {
            let kind = error_kind_name(self.code);
            core::write!(f, "Os {{ code: {}, kind: {}, message: {:?} }}", self.code, kind, self.description())
        }
    }
}
impl core::error::Error for Error {}

#[cfg(target_os = "macos")]
fn error_kind_name(code: i32) -> &'static str {
    match code {
        1 => "PermissionDenied",
        2 => "NotFound",
        4 => "Interrupted",
        5 => "InputOutputError",
        7 => "ArgumentListTooLong",
        11 => "Deadlock",
        12 => "OutOfMemory",
        13 => "PermissionDenied",
        16 => "ResourceBusy",
        17 => "AlreadyExists",
        18 => "CrossesDevices",
        20 => "NotADirectory",
        21 => "IsADirectory",
        22 => "InvalidInput",
        23 => "TooManyOpenFiles",
        24 => "TooManyOpenFiles",
        26 => "ExecutableFileBusy",
        27 => "FileTooLarge",
        28 => "StorageFull",
        29 => "NotSeekable",
        30 => "ReadOnlyFilesystem",
        31 => "TooManyLinks",
        32 => "BrokenPipe",
        35 => "WouldBlock",
        36 => "InProgress",
        48 => "AddrInUse",
        49 => "AddrNotAvailable",
        50 => "NetworkDown",
        51 => "NetworkUnreachable",
        53 => "ConnectionAborted",
        54 => "ConnectionReset",
        57 => "NotConnected",
        60 => "TimedOut",
        61 => "ConnectionRefused",
        62 => "FilesystemLoop",
        63 => "InvalidFilename",
        65 => "HostUnreachable",
        66 => "DirectoryNotEmpty",
        69 => "QuotaExceeded",
        70 => "StaleNetworkFileHandle",
        78 => "Unsupported",
        102 => "Unsupported",
        _ => "Uncategorized",
    }
}

#[cfg(target_os = "linux")]
fn error_kind_name(code: i32) -> &'static str {
    match code {
        1 => "PermissionDenied",
        2 => "NotFound",
        4 => "Interrupted",
        5 => "InputOutputError",
        7 => "ArgumentListTooLong",
        11 => "WouldBlock",
        12 => "OutOfMemory",
        13 => "PermissionDenied",
        16 => "ResourceBusy",
        17 => "AlreadyExists",
        18 => "CrossesDevices",
        20 => "NotADirectory",
        21 => "IsADirectory",
        22 => "InvalidInput",
        23 => "TooManyOpenFiles",
        24 => "TooManyOpenFiles",
        26 => "ExecutableFileBusy",
        27 => "FileTooLarge",
        28 => "StorageFull",
        29 => "NotSeekable",
        30 => "ReadOnlyFilesystem",
        31 => "TooManyLinks",
        32 => "BrokenPipe",
        35 => "Deadlock",
        36 => "InvalidFilename",
        38 => "Unsupported",
        39 => "DirectoryNotEmpty",
        40 => "FilesystemLoop",
        95 => "Unsupported",
        98 => "AddrInUse",
        99 => "AddrNotAvailable",
        100 => "NetworkDown",
        101 => "NetworkUnreachable",
        103 => "ConnectionAborted",
        104 => "ConnectionReset",
        107 => "NotConnected",
        110 => "TimedOut",
        111 => "ConnectionRefused",
        113 => "HostUnreachable",
        115 => "InProgress",
        116 => "StaleNetworkFileHandle",
        122 => "QuotaExceeded",
        _ => "Uncategorized",
    }
}
