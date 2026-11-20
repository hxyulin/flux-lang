//! Unix-domain stream sockets.
use crate::io::{Error, ErrorKind, Read, Result, Write};
use crate::os::fd::{AsRawFd, FromRawFd};
use crate::path::Path;

socket_owner!(UnixStream);

const AF_UNIX: i32 = 1;
#[cfg(target_os = "macos")]
const STREAM_KIND: i32 = 1;
#[cfg(target_os = "linux")]
const STREAM_KIND: i32 = 1 | 0o2000000; // SOCK_CLOEXEC
#[cfg(target_os = "macos")]
const PATH_CAPACITY: usize = 104;
#[cfg(target_os = "linux")]
const PATH_CAPACITY: usize = 108;

#[repr(C)]
struct UnixAddress {
    #[cfg(target_os = "macos")]
    len: u8,
    #[cfg(target_os = "macos")]
    family: u8,
    #[cfg(target_os = "linux")]
    family: u16,
    path: [u8; PATH_CAPACITY],
}

unsafe extern "C" {
    fn socket(domain: i32, kind: i32, protocol: i32) -> i32;
    fn socketpair(domain: i32, kind: i32, protocol: i32, fds: *mut i32) -> i32;
    fn connect(fd: i32, address: *const UnixAddress, len: u32) -> i32;
    fn shutdown(fd: i32, how: i32) -> i32;
    fn fcntl(fd: i32, command: i32, ...) -> i32;
    fn send(fd: i32, buffer: *const u8, len: usize, flags: i32) -> isize;
}

fn status(result: i32) -> Result<()> {
    if result < 0 {
        Err(Error::last_os_error())
    } else {
        Ok(())
    }
}

impl UnixStream {
    /// Connects to a Unix-domain socket at `path`.
    pub fn connect<P: AsRef<Path>>(path: P) -> Result<Self> {
        let bytes = path.as_ref().as_os_str().as_encoded_bytes();
        if bytes.len() >= PATH_CAPACITY || bytes.contains(&0) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid Unix socket path",
            ));
        }
        let len = (2 + bytes.len() + 1) as u32;
        let mut address = UnixAddress {
            #[cfg(target_os = "macos")]
            len: len as u8,
            family: AF_UNIX as _,
            path: [0; PATH_CAPACITY],
        };
        address.path[..bytes.len()].copy_from_slice(bytes);
        // SAFETY: socket creates a fresh stream descriptor for this process.
        let raw = unsafe { socket(AF_UNIX, STREAM_KIND, 0) };
        status(raw)?;
        // SAFETY: this takes the sole ownership of the new descriptor.
        let stream = unsafe { Self::from_raw_fd(raw) }.finish_creation()?;
        // SAFETY: the initialized address has `len` bytes and the socket is live.
        status(unsafe { connect(stream.as_raw_fd(), &address, len) })?;
        Ok(stream)
    }

    /// Creates two connected Unix-domain sockets.
    pub fn pair() -> Result<(Self, Self)> {
        let mut raw = [0; 2];
        // SAFETY: raw holds space for both resulting descriptors.
        status(unsafe { socketpair(AF_UNIX, STREAM_KIND, 0, raw.as_mut_ptr()) })?;
        // SAFETY: this takes ownership of both distinct new descriptors.
        let (a, b) = unsafe { (Self::from_raw_fd(raw[0]), Self::from_raw_fd(raw[1])) };
        Ok((a.finish_creation()?, b.finish_creation()?))
    }

    fn finish_creation(self) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            let fd = self.as_raw_fd();
            // SAFETY: the new descriptor is live. F_SETFD enables FD_CLOEXEC.
            status(unsafe { fcntl(fd, 2, 1i32) })?;
        }
        Ok(self)
    }

    /// Selects blocking or nonblocking operation.
    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
        #[cfg(target_os = "macos")]
        const NONBLOCK: i32 = 0x4;
        #[cfg(target_os = "linux")]
        const NONBLOCK: i32 = 0o4000;
        let fd = self.as_raw_fd();
        // SAFETY: this owner keeps the descriptor live. F_GETFL takes no argument.
        let flags = unsafe { fcntl(fd, 3) };
        status(flags)?;
        let flags = if nonblocking {
            flags | NONBLOCK
        } else {
            flags & !NONBLOCK
        };
        // SAFETY: F_SETFL expects the updated integer file status flags.
        status(unsafe { fcntl(fd, 4, flags) })
    }

    /// Shuts down the requested halves of the connection.
    pub fn shutdown(&self, how: crate::net::Shutdown) -> Result<()> {
        let how = match how {
            crate::net::Shutdown::Read => 0,
            crate::net::Shutdown::Write => 1,
            crate::net::Shutdown::Both => 2,
        };
        // SAFETY: the socket is live and how is a native shutdown selector.
        status(unsafe { shutdown(self.as_raw_fd(), how) })
    }
}

impl Read for &UnixStream {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        (&self.file).read(buffer)
    }
}

impl Read for UnixStream {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        (&*self).read(buffer)
    }
}

impl Write for &UnixStream {
    fn write(&mut self, buffer: &[u8]) -> Result<usize> {
        #[cfg(target_os = "linux")]
        const FLAGS: i32 = 0x4000; // MSG_NOSIGNAL
        #[cfg(target_os = "macos")]
        const FLAGS: i32 = 0x80000; // MSG_NOSIGNAL
        // SAFETY: the owned socket and all len bytes remain valid for send.
        let len = unsafe { send(self.as_raw_fd(), buffer.as_ptr(), buffer.len(), FLAGS) };
        if len < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(len as usize)
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Write for UnixStream {
    fn write(&mut self, buffer: &[u8]) -> Result<usize> {
        (&*self).write(buffer)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl core::fmt::Debug for UnixStream {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UnixStream")
            .field("fd", &self.as_raw_fd())
            .finish()
    }
}
