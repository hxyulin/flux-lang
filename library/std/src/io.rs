//! Byte-stream reading, writing, seeking, and filesystem errors.
use crate::alloc_crate::string::String;
use crate::alloc_crate::vec::Vec;

mod buffered;
pub use buffered::{BufRead, BufReader, BufWriter, Bytes, IntoInnerError, LineWriter, Lines, Split, Take, WriterPanicked};
mod stdio;
pub use stdio::{stdin, stdout, stderr, Stdin, StdinLock, Stdout, StdoutLock, Stderr, StderrLock, IsTerminal};

/// A writer that accepts and discards all bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sink;
pub const fn sink() -> Sink { Sink }
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> { Ok(bytes.len()) }
    fn flush(&mut self) -> Result<()> { Ok(()) }
}

pub type Result<T> = core::result::Result<T, Error>;

/// A borrowed byte buffer for vectored writes, with the Unix iovec layout.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct IoSlice<'a> { bytes: &'a [u8] }

impl<'a> IoSlice<'a> {
    pub fn new(bytes: &'a [u8]) -> Self { Self { bytes } }
    pub fn advance(&mut self, count: usize) { self.bytes = &self.bytes[count..]; }
    pub fn advance_slices(buffers: &mut &mut [Self], mut count: usize) {
        let mut consumed = 0;
        for buffer in buffers.iter() {
            if count < buffer.len() { break; }
            count -= buffer.len();
            consumed += 1;
        }
        *buffers = &mut core::mem::take(buffers)[consumed..];
        if buffers.is_empty() {
            assert!(count == 0, "advancing io slices beyond their length");
        } else {
            buffers[0].advance(count);
        }
    }
}
impl core::ops::Deref for IoSlice<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] { self.bytes }
}
impl core::fmt::Debug for IoSlice<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { core::fmt::Debug::fmt(self.bytes, f) }
}
pub struct Error { code: i32, message: Option<&'static str>, custom_kind: Option<ErrorKind>, custom_error: Option<crate::alloc_crate::boxed::Box<dyn core::error::Error + Send + Sync>> }
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
    pub fn new<E>(kind: ErrorKind, error: E) -> Self
        where E: Into<crate::alloc_crate::boxed::Box<dyn core::error::Error + Send + Sync>>
    {
        Self { code: 0, message: None, custom_kind: Some(kind), custom_error: Some(error.into()) }
    }
    pub fn get_ref(&self) -> Option<&(dyn core::error::Error + Send + Sync + 'static)> {
        self.custom_error.as_ref().map(|error| &**error)
    }
    pub fn get_mut(&mut self) -> Option<&mut (dyn core::error::Error + Send + Sync + 'static)> {
        self.custom_error.as_mut().map(|error| &mut **error)
    }
    pub fn into_inner(self) -> Option<crate::alloc_crate::boxed::Box<dyn core::error::Error + Send + Sync>> { self.custom_error }
    pub fn last_os_error() -> Error {
        // SAFETY: libc returns this thread's live errno slot.
        let code = unsafe {
            #[cfg(target_os = "macos")]
            { *__error() }
            #[cfg(target_os = "linux")]
            { *__errno_location() }
        };
        Error { custom_error: None, code, message: None, custom_kind: None }
    }
    pub fn from_raw_os_error(code: i32) -> Error { Error { custom_error: None, code, message: None, custom_kind: None } }
    pub fn raw_os_error(&self) -> Option<i32> { if self.custom_kind.is_none() { Some(self.code) } else { None } }
    /// A custom error of kind Other.
    pub fn other<E>(error: E) -> Error
        where E: Into<crate::alloc_crate::boxed::Box<dyn core::error::Error + Send + Sync>>
    {
        Error::new(ErrorKind::Other, error)
    }
    pub(crate) fn write_zero(message: &'static str) -> Error { Error { custom_error: None, code: 0, message: Some(message), custom_kind: Some(ErrorKind::WriteZero) } }
    pub(crate) fn nul_path() -> Error { Error { custom_error: None, code: 0, message: Some("file name contained an unexpected NUL byte"), custom_kind: Some(ErrorKind::InvalidInput) } }
    pub(crate) fn invalid_utf8() -> Error { Error { custom_error: None, code: 0, message: Some("stream did not contain valid UTF-8"), custom_kind: Some(ErrorKind::InvalidData) } }
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
        if let Some(error) = &self.custom_error { return core::fmt::Display::fmt(error, f); }
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
        if let Some(error) = &self.custom_error {
            return f.debug_struct("Custom").field("kind", &self.kind()).field("error", error).finish();
        }
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
impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.custom_error.as_ref().and_then(|error| error.source())
    }
}

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
        Error { custom_error: None, code: 0, message: None, custom_kind: Some(kind) }
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

/// A mutable byte buffer for vectored reads, with the Unix iovec layout.
#[repr(transparent)]
pub struct IoSliceMut<'a> { bytes: &'a mut [u8] }
impl<'a> IoSliceMut<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self { Self { bytes } }
    pub fn advance(&mut self, count: usize) {
        let bytes = core::mem::take(&mut self.bytes);
        self.bytes = &mut bytes[count..];
    }
    pub fn advance_slices(buffers: &mut &mut [Self], mut count: usize) {
        let mut consumed = 0;
        for buffer in buffers.iter() {
            if count < buffer.len() { break; }
            count -= buffer.len();
            consumed += 1;
        }
        *buffers = &mut core::mem::take(buffers)[consumed..];
        if buffers.is_empty() {
            assert!(count == 0, "advancing io slices beyond their length");
        } else {
            buffers[0].advance(count);
        }
    }
    pub fn into_slice(self) -> &'a mut [u8] { self.bytes }
}
impl core::ops::Deref for IoSliceMut<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] { self.bytes }
}
impl core::ops::DerefMut for IoSliceMut<'_> {
    fn deref_mut(&mut self) -> &mut [u8] { self.bytes }
}
impl core::fmt::Debug for IoSliceMut<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { core::fmt::Debug::fmt(&*self.bytes, f) }
}
impl<'a> IoSlice<'a> {
    pub fn as_slice(self) -> &'a [u8] { self.bytes }
}

/// A source of bytes. A successful read may return fewer bytes than requested.
pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Reads into the first nonempty buffer unless the reader overrides it.
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
        for buffer in bufs {
            if !buffer.is_empty() { return self.read(buffer); }
        }
        self.read(&mut [])
    }
    fn chain<R: Read>(self, next: R) -> Chain<Self, R> where Self: Sized { Chain { first: self, second: next, done_first: false } }

    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => return Err(Error { custom_error: None, code: 0, message: Some("failed to fill whole buffer"), custom_kind: Some(ErrorKind::UnexpectedEof) }),
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
    fn take(self, limit: u64) -> Take<Self> where Self: Sized { Take { inner: self, limit } }
    fn bytes(self) -> Bytes<Self> where Self: Sized { Bytes { inner: self } }
}

impl<R: Read + ?Sized> Read for &mut R {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> { (**self).read(buf) }
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> { (**self).read_vectored(bufs) }
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
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
        let mut total = 0;
        for buf in bufs {
            total += self.read(buf)?;
            if self.is_empty() { break; }
        }
        Ok(total)
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

    /// Writes from the first nonempty buffer unless the writer overrides it.
    fn write_vectored(&mut self, buffers: &[IoSlice<'_>]) -> Result<usize> {
        for buffer in buffers {
            if !buffer.is_empty() { return self.write(buffer); }
        }
        self.write(&[])
    }

    fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => return Err(Error { custom_error: None, code: 0, message: Some("failed to write whole buffer"), custom_kind: Some(ErrorKind::WriteZero) }),
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
    fn write_vectored(&mut self, buffers: &[IoSlice<'_>]) -> Result<usize> { (**self).write_vectored(buffers) }
    fn write_all(&mut self, buf: &[u8]) -> Result<()> { (**self).write_all(buf) }
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<()> { (**self).write_fmt(args) }
}
impl<W: Write + ?Sized> Write for crate::alloc_crate::boxed::Box<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { (**self).write(buf) }
    fn flush(&mut self) -> Result<()> { (**self).flush() }
    fn write_vectored(&mut self, buffers: &[IoSlice<'_>]) -> Result<usize> { (**self).write_vectored(buffers) }
    fn write_all(&mut self, buf: &[u8]) -> Result<()> { (**self).write_all(buf) }
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<()> { (**self).write_fmt(args) }
}
impl Write for Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { self.extend_from_slice(buf); Ok(buf.len()) }
    fn flush(&mut self) -> Result<()> { Ok(()) }
    fn write_vectored(&mut self, buffers: &[IoSlice<'_>]) -> Result<usize> {
        let mut length = 0usize;
        for buffer in buffers { length = length.checked_add(buffer.len()).expect("vectored write length overflow"); }
        self.reserve(length);
        for buffer in buffers { self.extend_from_slice(buffer); }
        Ok(length)
    }
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

/// An in-memory byte stream with an independent position.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cursor<T> {
    inner: T,
    pos: u64,
}

impl<T> Cursor<T> {
    pub const fn new(inner: T) -> Cursor<T> { Cursor { inner, pos: 0 } }
    pub const fn get_ref(&self) -> &T { &self.inner }
    pub const fn get_mut(&mut self) -> &mut T { &mut self.inner }
    pub fn into_inner(self) -> T { self.inner }
    pub const fn position(&self) -> u64 { self.pos }
    pub const fn set_position(&mut self, pos: u64) { self.pos = pos; }
}

impl<T: AsRef<[u8]>> Read for Cursor<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let bytes = self.inner.as_ref();
        let start = core::cmp::min(self.pos, bytes.len() as u64) as usize;
        let count = core::cmp::min(buf.len(), bytes.len() - start);
        buf[..count].copy_from_slice(&bytes[start..start + count]);
        self.pos += count as u64;
        Ok(count)
    }
}

impl<T: AsRef<[u8]>> Seek for Cursor<T> {
    fn seek(&mut self, from: SeekFrom) -> Result<u64> {
        let (base, offset) = match from {
            SeekFrom::Start(pos) => { self.pos = pos; return Ok(pos); }
            SeekFrom::End(offset) => (self.inner.as_ref().len() as u64, offset),
            SeekFrom::Current(offset) => (self.pos, offset),
        };
        let next = if offset >= 0 {
            base.checked_add(offset as u64)
        } else {
            base.checked_sub(offset.wrapping_neg() as u64)
        };
        match next {
            Some(pos) => { self.pos = pos; Ok(pos) }
            None => Err(cursor_invalid("invalid seek to a negative or overflowing position")),
        }
    }
    fn stream_position(&mut self) -> Result<u64> { Ok(self.pos) }
}

fn cursor_invalid(message: &'static str) -> Error {
    Error { custom_error: None, code: 0, message: Some(message), custom_kind: Some(ErrorKind::InvalidInput) }
}

fn cursor_write_vec(pos: &mut u64, bytes: &mut Vec<u8>, buf: &[u8]) -> Result<usize> {
    let start = usize::try_from(*pos).map_err(|_| cursor_invalid("cursor position exceeds maximum possible vector length"))?;
    let end = start.checked_add(buf.len()).ok_or_else(|| cursor_invalid("cursor position exceeds maximum possible vector length"))?;
    if end > bytes.len() { bytes.resize(end, 0); }
    bytes[start..end].copy_from_slice(buf);
    *pos += buf.len() as u64;
    Ok(buf.len())
}

impl Write for Cursor<Vec<u8>> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { cursor_write_vec(&mut self.pos, &mut self.inner, buf) }
    fn flush(&mut self) -> Result<()> { Ok(()) }
    fn write_all(&mut self, buf: &[u8]) -> Result<()> { self.write(buf).map(|_| ()) }
}
impl Write for Cursor<&mut Vec<u8>> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { cursor_write_vec(&mut self.pos, self.inner, buf) }
    fn flush(&mut self) -> Result<()> { Ok(()) }
    fn write_all(&mut self, buf: &[u8]) -> Result<()> { self.write(buf).map(|_| ()) }
}

fn cursor_write_slice(pos: &mut u64, bytes: &mut [u8], buf: &[u8]) -> Result<usize> {
    let start = core::cmp::min(*pos, bytes.len() as u64) as usize;
    let count = core::cmp::min(buf.len(), bytes.len() - start);
    bytes[start..start + count].copy_from_slice(&buf[..count]);
    *pos += count as u64;
    Ok(count)
}
impl Write for Cursor<&mut [u8]> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { cursor_write_slice(&mut self.pos, self.inner, buf) }
    fn flush(&mut self) -> Result<()> { Ok(()) }
}
impl<const N: usize> Write for Cursor<[u8; N]> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { cursor_write_slice(&mut self.pos, &mut self.inner, buf) }
    fn flush(&mut self) -> Result<()> { Ok(()) }
}
impl Write for Cursor<crate::alloc_crate::boxed::Box<[u8]>> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { cursor_write_slice(&mut self.pos, &mut self.inner, buf) }
    fn flush(&mut self) -> Result<()> { Ok(()) }
}

/// Reads one source to its end, then the next.
#[derive(Debug)]
pub struct Chain<T, U> { first: T, second: U, done_first: bool }
impl<T, U> Chain<T, U> {
    pub fn into_inner(self) -> (T, U) { (self.first, self.second) }
    pub fn get_ref(&self) -> (&T, &U) { (&self.first, &self.second) }
    pub fn get_mut(&mut self) -> (&mut T, &mut U) { (&mut self.first, &mut self.second) }
}
impl<T: Read, U: Read> Read for Chain<T, U> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.done_first {
            match self.first.read(buf)? {
                0 if !buf.is_empty() => self.done_first = true,
                n => return Ok(n),
            }
        }
        self.second.read(buf)
    }
}
impl<T: BufRead, U: BufRead> BufRead for Chain<T, U> {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        if !self.done_first {
            match self.first.fill_buf()? {
                buf if buf.is_empty() => self.done_first = true,
                buf => return Ok(buf),
            }
        }
        self.second.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        if !self.done_first { self.first.consume(amt) } else { self.second.consume(amt) }
    }
}

/// A reader that is always at its end.
#[derive(Clone, Copy, Debug, Default)]
pub struct Empty;
pub const fn empty() -> Empty { Empty }
impl Read for Empty {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize> { Ok(0) }
}
impl BufRead for Empty {
    fn fill_buf(&mut self) -> Result<&[u8]> { Ok(&[]) }
    fn consume(&mut self, _amt: usize) {}
}
impl Write for Empty {
    fn write(&mut self, buf: &[u8]) -> Result<usize> { Ok(buf.len()) }
    fn flush(&mut self) -> Result<()> { Ok(()) }
}
impl Seek for Empty {
    fn seek(&mut self, _pos: SeekFrom) -> Result<u64> { Ok(0) }
}

/// A reader that yields one byte forever.
#[derive(Debug)]
pub struct Repeat { byte: u8 }
pub const fn repeat(byte: u8) -> Repeat { Repeat { byte } }
impl Read for Repeat {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        for slot in &mut *buf { *slot = self.byte; }
        Ok(buf.len())
    }
}

/// Copies the reader to its end into the writer, returning the byte count.
pub fn copy<R: Read + ?Sized, W: Write + ?Sized>(reader: &mut R, writer: &mut W) -> Result<u64> {
    let mut buffer = [0u8; 8192];
    let mut total = 0u64;
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        writer.write_all(&buffer[..count])?;
        total += count as u64;
    }
}

/// Reads the whole source into a new string.
pub fn read_to_string<R: Read>(mut reader: R) -> Result<String> {
    let mut buf = String::new();
    reader.read_to_string(&mut buf)?;
    Ok(buf)
}

/// The traits most byte-stream code uses.
pub mod prelude {
    pub use super::{BufRead, Read, Seek, Write};
}
