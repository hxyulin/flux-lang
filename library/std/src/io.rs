//! Byte-stream reading, writing, seeking, and filesystem errors.
use crate::alloc_crate::string::String;
use crate::alloc_crate::vec::Vec;

pub type Result<T> = core::result::Result<T, Error>;
pub struct Error { code: i32, message: Option<&'static str>, custom_kind: Option<ErrorKind> }
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
        Error { code, message: None, custom_kind: None }
    }
    pub fn from_raw_os_error(code: i32) -> Error { Error { code, message: None, custom_kind: None } }
    pub fn raw_os_error(&self) -> Option<i32> { if self.custom_kind.is_none() { Some(self.code) } else { None } }
    pub(crate) fn nul_path() -> Error { Error { code: 0, message: Some("file name contained an unexpected NUL byte"), custom_kind: Some(ErrorKind::InvalidInput) } }
    pub(crate) fn invalid_utf8() -> Error { Error { code: 0, message: Some("stream did not contain valid UTF-8"), custom_kind: Some(ErrorKind::InvalidData) } }
    pub fn kind(&self) -> ErrorKind { self.custom_kind.unwrap_or_else(|| error_kind(self.code)) }
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
            None => match self.custom_kind {
                Some(kind) => f.write_str(kind.description()),
                None => core::write!(f, "{} (os error {})", self.description(), self.code),
            },
        }
    }
}
impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(message) = self.message {
            core::write!(f, "Error {{ kind: {:?}, message: {:?} }}", self.kind(), message)
        } else if let Some(kind) = self.custom_kind {
            core::write!(f, "Kind({:?})", kind)
        } else {
            let kind = self.kind();
            core::write!(f, "Os {{ code: {}, kind: {:?}, message: {:?} }}", self.code, kind, self.description())
        }
    }
}
impl core::error::Error for Error {}

#[cfg(target_os = "macos")]
fn error_kind(code: i32) -> ErrorKind {
    match code {
        1 => ErrorKind::PermissionDenied,
        2 => ErrorKind::NotFound,
        4 => ErrorKind::Interrupted,
        5 => ErrorKind::InputOutputError,
        7 => ErrorKind::ArgumentListTooLong,
        11 => ErrorKind::Deadlock,
        12 => ErrorKind::OutOfMemory,
        13 => ErrorKind::PermissionDenied,
        16 => ErrorKind::ResourceBusy,
        17 => ErrorKind::AlreadyExists,
        18 => ErrorKind::CrossesDevices,
        20 => ErrorKind::NotADirectory,
        21 => ErrorKind::IsADirectory,
        22 => ErrorKind::InvalidInput,
        23 => ErrorKind::TooManyOpenFiles,
        24 => ErrorKind::TooManyOpenFiles,
        26 => ErrorKind::ExecutableFileBusy,
        27 => ErrorKind::FileTooLarge,
        28 => ErrorKind::StorageFull,
        29 => ErrorKind::NotSeekable,
        30 => ErrorKind::ReadOnlyFilesystem,
        31 => ErrorKind::TooManyLinks,
        32 => ErrorKind::BrokenPipe,
        35 => ErrorKind::WouldBlock,
        36 => ErrorKind::InProgress,
        48 => ErrorKind::AddrInUse,
        49 => ErrorKind::AddrNotAvailable,
        50 => ErrorKind::NetworkDown,
        51 => ErrorKind::NetworkUnreachable,
        53 => ErrorKind::ConnectionAborted,
        54 => ErrorKind::ConnectionReset,
        57 => ErrorKind::NotConnected,
        60 => ErrorKind::TimedOut,
        61 => ErrorKind::ConnectionRefused,
        62 => ErrorKind::FilesystemLoop,
        63 => ErrorKind::InvalidFilename,
        65 => ErrorKind::HostUnreachable,
        66 => ErrorKind::DirectoryNotEmpty,
        69 => ErrorKind::QuotaExceeded,
        70 => ErrorKind::StaleNetworkFileHandle,
        78 => ErrorKind::Unsupported,
        102 => ErrorKind::Unsupported,
        _ => ErrorKind::Uncategorized,
    }
}

#[cfg(target_os = "linux")]
fn error_kind(code: i32) -> ErrorKind {
    match code {
        1 => ErrorKind::PermissionDenied,
        2 => ErrorKind::NotFound,
        4 => ErrorKind::Interrupted,
        5 => ErrorKind::InputOutputError,
        7 => ErrorKind::ArgumentListTooLong,
        11 => ErrorKind::WouldBlock,
        12 => ErrorKind::OutOfMemory,
        13 => ErrorKind::PermissionDenied,
        16 => ErrorKind::ResourceBusy,
        17 => ErrorKind::AlreadyExists,
        18 => ErrorKind::CrossesDevices,
        20 => ErrorKind::NotADirectory,
        21 => ErrorKind::IsADirectory,
        22 => ErrorKind::InvalidInput,
        23 => ErrorKind::TooManyOpenFiles,
        24 => ErrorKind::TooManyOpenFiles,
        26 => ErrorKind::ExecutableFileBusy,
        27 => ErrorKind::FileTooLarge,
        28 => ErrorKind::StorageFull,
        29 => ErrorKind::NotSeekable,
        30 => ErrorKind::ReadOnlyFilesystem,
        31 => ErrorKind::TooManyLinks,
        32 => ErrorKind::BrokenPipe,
        35 => ErrorKind::Deadlock,
        36 => ErrorKind::InvalidFilename,
        38 => ErrorKind::Unsupported,
        39 => ErrorKind::DirectoryNotEmpty,
        40 => ErrorKind::FilesystemLoop,
        95 => ErrorKind::Unsupported,
        98 => ErrorKind::AddrInUse,
        99 => ErrorKind::AddrNotAvailable,
        100 => ErrorKind::NetworkDown,
        101 => ErrorKind::NetworkUnreachable,
        103 => ErrorKind::ConnectionAborted,
        104 => ErrorKind::ConnectionReset,
        107 => ErrorKind::NotConnected,
        110 => ErrorKind::TimedOut,
        111 => ErrorKind::ConnectionRefused,
        113 => ErrorKind::HostUnreachable,
        115 => ErrorKind::InProgress,
        116 => ErrorKind::StaleNetworkFileHandle,
        122 => ErrorKind::QuotaExceeded,
        _ => ErrorKind::Uncategorized,
    }
}

/// A category describing why an I/O operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    PermissionDenied,
    NotFound,
    Interrupted,
    InputOutputError,
    ArgumentListTooLong,
    Deadlock,
    OutOfMemory,
    ResourceBusy,
    AlreadyExists,
    CrossesDevices,
    NotADirectory,
    IsADirectory,
    InvalidInput,
    TooManyOpenFiles,
    ExecutableFileBusy,
    FileTooLarge,
    StorageFull,
    NotSeekable,
    ReadOnlyFilesystem,
    TooManyLinks,
    BrokenPipe,
    WouldBlock,
    InProgress,
    AddrInUse,
    AddrNotAvailable,
    NetworkDown,
    NetworkUnreachable,
    ConnectionAborted,
    ConnectionReset,
    NotConnected,
    TimedOut,
    ConnectionRefused,
    FilesystemLoop,
    InvalidFilename,
    HostUnreachable,
    DirectoryNotEmpty,
    QuotaExceeded,
    StaleNetworkFileHandle,
    Unsupported,
    Uncategorized,
    UnexpectedEof,
    WriteZero,
    InvalidData,
    Other,
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error { code: 0, message: None, custom_kind: Some(kind) }
    }
}

impl ErrorKind {
    fn description(self) -> &'static str {
        match self {
            ErrorKind::AddrInUse => "address in use",
            ErrorKind::AddrNotAvailable => "address not available",
            ErrorKind::AlreadyExists => "entity already exists",
            ErrorKind::ArgumentListTooLong => "argument list too long",
            ErrorKind::BrokenPipe => "broken pipe",
            ErrorKind::ConnectionAborted => "connection aborted",
            ErrorKind::ConnectionRefused => "connection refused",
            ErrorKind::ConnectionReset => "connection reset",
            ErrorKind::CrossesDevices => "cross-device link or rename",
            ErrorKind::Deadlock => "deadlock",
            ErrorKind::DirectoryNotEmpty => "directory not empty",
            ErrorKind::ExecutableFileBusy => "executable file busy",
            ErrorKind::FileTooLarge => "file too large",
            ErrorKind::FilesystemLoop => "filesystem loop or indirection limit (e.g. symlink loop)",
            ErrorKind::HostUnreachable => "host unreachable",
            ErrorKind::InProgress => "in progress",
            ErrorKind::InputOutputError => "input/output error",
            ErrorKind::Interrupted => "operation interrupted",
            ErrorKind::InvalidData => "invalid data",
            ErrorKind::InvalidFilename => "invalid filename",
            ErrorKind::InvalidInput => "invalid input parameter",
            ErrorKind::IsADirectory => "is a directory",
            ErrorKind::NetworkDown => "network down",
            ErrorKind::NetworkUnreachable => "network unreachable",
            ErrorKind::NotADirectory => "not a directory",
            ErrorKind::NotConnected => "not connected",
            ErrorKind::NotFound => "entity not found",
            ErrorKind::NotSeekable => "seek on unseekable file",
            ErrorKind::Other => "other error",
            ErrorKind::OutOfMemory => "out of memory",
            ErrorKind::PermissionDenied => "permission denied",
            ErrorKind::QuotaExceeded => "quota exceeded",
            ErrorKind::ReadOnlyFilesystem => "read-only filesystem or storage medium",
            ErrorKind::ResourceBusy => "resource busy",
            ErrorKind::StaleNetworkFileHandle => "stale network file handle",
            ErrorKind::StorageFull => "no storage space",
            ErrorKind::TimedOut => "timed out",
            ErrorKind::TooManyLinks => "too many links",
            ErrorKind::TooManyOpenFiles => "too many open files",
            ErrorKind::Uncategorized => "uncategorized error",
            ErrorKind::UnexpectedEof => "unexpected end of file",
            ErrorKind::Unsupported => "unsupported",
            ErrorKind::WouldBlock => "operation would block",
            ErrorKind::WriteZero => "write zero",
        }
    }
}

/// A source of bytes. A successful read may return fewer bytes than requested.
pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => return Err(Error { code: 0, message: Some("failed to fill whole buffer"), custom_kind: Some(ErrorKind::UnexpectedEof) }),
                Ok(count) => { buf = &mut buf[count..]; }
                Err(error) => {
                    if error.kind() != ErrorKind::Interrupted { return Err(error); }
                }
            }
        }
        Ok(())
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        let mut total = 0;
        let mut chunk = [0u8; 8192];
        loop {
            match self.read(&mut chunk) {
                Ok(0) => return Ok(total),
                Ok(count) => { buf.extend_from_slice(&chunk[..count]); total += count; }
                Err(error) => {
                    if error.kind() != ErrorKind::Interrupted { return Err(error); }
                }
            }
        }
    }

    fn read_to_string(&mut self, buf: &mut String) -> Result<usize> {
        let mut bytes = Vec::new();
        let result = self.read_to_end(&mut bytes);
        match String::from_utf8(bytes) {
            Ok(text) => { buf.push_str(&text); result }
            Err(_) => match result { Err(error) => Err(error), Ok(_) => Err(Error::invalid_utf8()) },
        }
    }

    fn by_ref(&mut self) -> &mut Self where Self: Sized { self }
}

impl<R: Read + ?Sized> Read for &mut R {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> { (**self).read(buf) }
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> { (**self).read_exact(buf) }
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> { (**self).read_to_end(buf) }
    fn read_to_string(&mut self, buf: &mut String) -> Result<usize> { (**self).read_to_string(buf) }
}

impl Read for &[u8] {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let count = core::cmp::min(buf.len(), self.len());
        buf[..count].copy_from_slice(&self[..count]);
        *self = &self[count..];
        Ok(count)
    }
}

/// The origin and offset of a seek operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekFrom { Start(u64), End(i64), Current(i64) }

/// A byte stream whose position can be changed.
pub trait Seek {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
    fn rewind(&mut self) -> Result<()> { self.seek(SeekFrom::Start(0)).map(|_| ()) }
    fn stream_position(&mut self) -> Result<u64> { self.seek(SeekFrom::Current(0)) }
}
impl<S: Seek + ?Sized> Seek for &mut S {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> { (**self).seek(pos) }
    fn rewind(&mut self) -> Result<()> { (**self).rewind() }
    fn stream_position(&mut self) -> Result<u64> { (**self).stream_position() }
}


/// A destination for bytes. A successful write may consume only part of a buffer.
pub trait Write {
    /// Writes up to `buf.len()` bytes and returns the number consumed.
    /// A successful count must not exceed the input length; zero means no progress.
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    /// Sends buffered output to its destination.
    fn flush(&mut self) -> Result<()>;

    fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => return Err(Error { code: 0, message: Some("failed to write whole buffer"), custom_kind: Some(ErrorKind::WriteZero) }),
                Ok(count) => { buf = &buf[count..]; }
                Err(error) => {
                    if error.kind() != ErrorKind::Interrupted { return Err(error); }
                }
            }
        }
        Ok(())
    }

    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<()> {
        let mut adapter = WriteAdapter { writer: self, error: None };
        match core::fmt::write(&mut adapter, args) {
            Ok(()) => Ok(()),
            Err(_) => match adapter.error {
                Some(error) => Err(error),
                None => panic!("a formatting trait implementation returned an error when the underlying stream did not"),
            },
        }
    }

    fn by_ref(&mut self) -> &mut Self where Self: Sized { self }
}

struct WriteAdapter<'a, W: Write + ?Sized> {
    writer: &'a mut W,
    error: Option<Error>,
}
impl<W: Write + ?Sized> core::fmt::Write for WriteAdapter<'_, W> {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        match self.writer.write_all(text.as_bytes()) {
            Ok(()) => Ok(()),
            Err(error) => { self.error = Some(error); Err(core::fmt::Error) }
        }
    }
}

impl<W: Write + ?Sized> Write for &mut W {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { (**self).write(buf) }
    fn flush(&mut self) -> Result<()> { (**self).flush() }
    fn write_all(&mut self, buf: &[u8]) -> Result<()> { (**self).write_all(buf) }
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<()> { (**self).write_fmt(args) }
}
impl<W: Write + ?Sized> Write for crate::alloc_crate::boxed::Box<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { (**self).write(buf) }
    fn flush(&mut self) -> Result<()> { (**self).flush() }
    fn write_all(&mut self, buf: &[u8]) -> Result<()> { (**self).write_all(buf) }
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<()> { (**self).write_fmt(args) }
}
impl Write for Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { self.extend_from_slice(buf); Ok(buf.len()) }
    fn flush(&mut self) -> Result<()> { Ok(()) }
    fn write_all(&mut self, buf: &[u8]) -> Result<()> { self.extend_from_slice(buf); Ok(()) }
}
impl Write for &mut [u8] {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let count = core::cmp::min(buf.len(), self.len());
        let bytes = core::mem::take(self);
        let (written, remaining) = bytes.split_at_mut(count);
        written.copy_from_slice(&buf[..count]);
        *self = remaining;
        Ok(count)
    }
    fn flush(&mut self) -> Result<()> { Ok(()) }
}
