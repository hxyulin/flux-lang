//! Unix-domain sockets.
use super::native;
use crate::io::{Error, ErrorKind, Read, Result, Write};
use crate::os::fd::AsRawFd;
use crate::os::unix::net::SocketAddr;
use crate::path::Path;

socket_owner!(UnixStream);
socket_owner!(UnixListener);
socket_owner!(UnixDatagram);

const AF_UNIX: i32 = 1;
const STREAM: i32 = 1;
const DATAGRAM: i32 = 2;
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

impl UnixAddress {
    fn empty() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            len: 0,
            family: AF_UNIX as _,
            path: [0; PATH_CAPACITY],
        }
    }

    fn at(path: &Path) -> Result<(Self, u32)> {
        let bytes = path.as_os_str().as_encoded_bytes();
        if bytes.len() >= PATH_CAPACITY || bytes.contains(&0) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid Unix socket path",
            ));
        }
        let len = if bytes.is_empty() {
            2
        } else {
            3 + bytes.len() as u32
        };
        let mut address = Self::empty();
        #[cfg(target_os = "macos")]
        {
            address.len = len as u8;
        }
        address.path[..bytes.len()].copy_from_slice(bytes);
        Ok((address, len))
    }

    fn capacity() -> u32 {
        core::mem::size_of::<Self>() as u32
    }

    fn as_ptr(&self) -> *const u8 {
        self as *const Self as *const u8
    }
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self as *mut Self as *mut u8
    }

    fn decode(&self, len: u32) -> SocketAddr {
        let bytes = &self.path[..(len.saturating_sub(2) as usize).min(PATH_CAPACITY)];
        #[cfg(target_os = "linux")]
        if let Some((&0, name)) = bytes.split_first() {
            return SocketAddr::from_native(name, true);
        }
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        SocketAddr::from_native(&bytes[..end], false)
    }
}

fn bind(fd: i32, path: &Path) -> Result<()> {
    let (address, len) = UnixAddress::at(path)?;
    // SAFETY: the owner keeps fd live and this address has len initialized bytes.
    native::status(unsafe { native::bind(fd, address.as_ptr(), len) })
}

fn connect(fd: i32, path: &Path) -> Result<()> {
    let (address, len) = UnixAddress::at(path)?;
    // SAFETY: the owner keeps fd live and this address has len initialized bytes.
    native::status(unsafe { native::connect(fd, address.as_ptr(), len) })
}

fn socket_address(fd: i32, peer: bool) -> Result<SocketAddr> {
    let mut address = UnixAddress::empty();
    let mut len = UnixAddress::capacity();
    // SAFETY: fd stays live; address and len are valid for native output.
    let status = unsafe {
        if peer {
            native::getpeername(fd, address.as_mut_ptr(), &mut len)
        } else {
            native::getsockname(fd, address.as_mut_ptr(), &mut len)
        }
    };
    native::status(status)?;
    Ok(address.decode(len))
}

macro_rules! socket_methods {
    ($name:ident) => {
        impl $name {
            /// Selects blocking or nonblocking operation.
            pub fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
                native::set_nonblocking(self.as_raw_fd(), nonblocking)
            }

            /// Returns the socket's local address.
            pub fn local_addr(&self) -> Result<SocketAddr> {
                socket_address(self.as_raw_fd(), false)
            }
        }
        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("fd", &self.as_raw_fd())
                    .finish()
            }
        }
    };
}
socket_methods!(UnixStream);
socket_methods!(UnixListener);
socket_methods!(UnixDatagram);

impl UnixStream {
    /// Connects to a Unix-domain socket at `path`.
    pub fn connect<P: AsRef<Path>>(path: P) -> Result<Self> {
        let stream = Self {
            file: native::socket_file(AF_UNIX, STREAM)?,
        };
        connect(stream.as_raw_fd(), path.as_ref())?;
        Ok(stream)
    }

    /// Creates two connected Unix-domain stream sockets.
    pub fn pair() -> Result<(Self, Self)> {
        let (a, b) = native::socket_pair(AF_UNIX, STREAM)?;
        Ok((Self { file: a }, Self { file: b }))
    }

    /// Returns the connected peer's address.
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        socket_address(self.as_raw_fd(), true)
    }

    /// Shuts down the requested halves of the connection.
    pub fn shutdown(&self, how: crate::net::Shutdown) -> Result<()> {
        let how = match how {
            crate::net::Shutdown::Read => 0,
            crate::net::Shutdown::Write => 1,
            crate::net::Shutdown::Both => 2,
        };
        // SAFETY: the owned descriptor stays live and how is a native selector.
        native::status(unsafe { native::shutdown(self.as_raw_fd(), how) })
    }
}

impl UnixListener {
    /// Binds and listens on the Unix-domain socket at `path`.
    pub fn bind<P: AsRef<Path>>(path: P) -> Result<Self> {
        let listener = Self {
            file: native::socket_file(AF_UNIX, STREAM)?,
        };
        bind(listener.as_raw_fd(), path.as_ref())?;
        // SAFETY: the newly bound stream socket stays live.
        native::status(unsafe { native::listen(listener.as_raw_fd(), 128) })?;
        Ok(listener)
    }

    /// Accepts a new connection and returns the stream and peer address.
    pub fn accept(&self) -> Result<(UnixStream, SocketAddr)> {
        let mut address = UnixAddress::empty();
        let mut len = UnixAddress::capacity();
        // SAFETY: this listener and the two output locations stay live.
        let file =
            unsafe { native::accept_file(self.as_raw_fd(), address.as_mut_ptr(), &mut len) }?;
        Ok((UnixStream { file }, address.decode(len)))
    }
}

impl UnixDatagram {
    /// Creates a socket with no local address.
    pub fn unbound() -> Result<Self> {
        Ok(Self {
            file: native::socket_file(AF_UNIX, DATAGRAM)?,
        })
    }

    /// Binds a datagram socket to the given path.
    pub fn bind<P: AsRef<Path>>(path: P) -> Result<Self> {
        let socket = Self::unbound()?;
        bind(socket.as_raw_fd(), path.as_ref())?;
        Ok(socket)
    }

    /// Creates two connected Unix-domain datagram sockets.
    pub fn pair() -> Result<(Self, Self)> {
        let (a, b) = native::socket_pair(AF_UNIX, DATAGRAM)?;
        Ok((Self { file: a }, Self { file: b }))
    }

    /// Selects the remote socket for subsequent sends and receives.
    pub fn connect<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        connect(self.as_raw_fd(), path.as_ref())
    }

    /// Returns the connected peer's address.
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        socket_address(self.as_raw_fd(), true)
    }

    /// Receives a datagram and its sender's address.
    pub fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)> {
        let mut address = UnixAddress::empty();
        let mut len = UnixAddress::capacity();
        // SAFETY: the socket and all output buffers stay live and writable.
        let count = native::byte_count(unsafe {
            native::recvfrom(
                self.as_raw_fd(),
                buffer.as_mut_ptr(),
                buffer.len(),
                0,
                address.as_mut_ptr(),
                &mut len,
            )
        })?;
        Ok((count, address.decode(len)))
    }

    /// Sends a datagram to the given path.
    pub fn send_to<P: AsRef<Path>>(&self, buffer: &[u8], path: P) -> Result<usize> {
        let (address, len) = UnixAddress::at(path.as_ref())?;
        // SAFETY: the socket and both initialized input buffers stay live.
        native::byte_count(unsafe {
            native::sendto(
                self.as_raw_fd(),
                buffer.as_ptr(),
                buffer.len(),
                native::NO_SIGNAL,
                address.as_ptr(),
                len,
            )
        })
    }

    /// Receives a datagram from the connected peer.
    pub fn recv(&self, buffer: &mut [u8]) -> Result<usize> {
        // SAFETY: this socket and the writable buffer stay live.
        native::byte_count(unsafe {
            native::recv(self.as_raw_fd(), buffer.as_mut_ptr(), buffer.len(), 0)
        })
    }

    /// Sends a datagram to the connected peer.
    pub fn send(&self, buffer: &[u8]) -> Result<usize> {
        // SAFETY: this socket and the readable buffer stay live.
        native::byte_count(unsafe {
            native::send(
                self.as_raw_fd(),
                buffer.as_ptr(),
                buffer.len(),
                native::NO_SIGNAL,
            )
        })
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
        // SAFETY: this owner and the readable buffer are live for send.
        native::byte_count(unsafe {
            native::send(
                self.as_raw_fd(),
                buffer.as_ptr(),
                buffer.len(),
                native::NO_SIGNAL,
            )
        })
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
