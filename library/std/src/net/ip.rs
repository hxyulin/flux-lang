//! TCP and UDP sockets backed by the native IPv4 and IPv6 interfaces.
use super::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, native};
use super::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use crate::fs::File;
use crate::io::{Error, ErrorKind, Read, Result, Write};
use crate::os::fd::AsRawFd;

const AF_INET: i32 = 2;
#[cfg(target_os = "macos")]
const AF_INET6: i32 = 30;
#[cfg(target_os = "linux")]
const AF_INET6: i32 = 10;
const STREAM: i32 = 1;
const DATAGRAM: i32 = 2;
const INET_LEN: usize = 16;
const INET6_LEN: usize = 28;

#[repr(align(8))]
struct Address([u8; INET6_LEN]);

impl Address {
    fn encode(addr: SocketAddr) -> (i32, Self, u32) {
        let mut bytes = [0; INET6_LEN];
        bytes[2..4].copy_from_slice(&addr.port().to_be_bytes());
        let (family, len) = match addr {
            SocketAddr::V4(addr) => {
                bytes[4..8].copy_from_slice(&addr.ip().octets());
                (AF_INET, INET_LEN)
            }
            SocketAddr::V6(addr) => {
                bytes[4..8].copy_from_slice(&addr.flowinfo().to_ne_bytes());
                bytes[8..24].copy_from_slice(&addr.ip().octets());
                bytes[24..28].copy_from_slice(&addr.scope_id().to_ne_bytes());
                (AF_INET6, INET6_LEN)
            }
        };
        #[cfg(target_os = "macos")]
        {
            bytes[0] = len as u8;
            bytes[1] = family as u8;
        }
        #[cfg(target_os = "linux")]
        bytes[..2].copy_from_slice(&(family as u16).to_ne_bytes());
        (family, Self(bytes), len as u32)
    }

    fn decode(&self, len: u32) -> Result<SocketAddr> {
        decode_address(&self.0[..(len as usize).min(self.0.len())])
    }
}

pub(super) fn decode_address(bytes: &[u8]) -> Result<SocketAddr> {
    if bytes.len() < 4 {
        return Err(invalid_address());
    }
    #[cfg(target_os = "macos")]
    let family = bytes[1] as i32;
    #[cfg(target_os = "linux")]
    let family = u16::from_ne_bytes([bytes[0], bytes[1]]) as i32;
    let port = u16::from_be_bytes([bytes[2], bytes[3]]);
    match family {
        AF_INET if bytes.len() >= INET_LEN => {
            let ip = Ipv4Addr::new(bytes[4], bytes[5], bytes[6], bytes[7]);
            Ok(SocketAddr::V4(SocketAddrV4::new(ip, port)))
        }
        AF_INET6 if bytes.len() >= INET6_LEN => {
            let mut ip = [0; 16];
            ip.copy_from_slice(&bytes[8..24]);
            let flow = u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            let scope = u32::from_ne_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
            Ok(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(ip),
                port,
                flow,
                scope,
            )))
        }
        _ => Err(invalid_address()),
    }
}

fn invalid_address() -> Error {
    Error::new(ErrorKind::InvalidInput, "invalid IP socket address")
}

fn try_addresses<A: ToSocketAddrs, T>(
    addresses: A,
    mut operation: impl FnMut(SocketAddr) -> Result<T>,
) -> Result<T> {
    let mut error = no_addresses();
    for address in addresses.to_socket_addrs()? {
        match operation(address) {
            Ok(result) => return Ok(result),
            Err(failure) => error = failure,
        }
    }
    Err(error)
}

fn no_addresses() -> Error {
    Error::new(
        ErrorKind::InvalidInput,
        "could not resolve to any addresses",
    )
}

fn bind_file(addr: SocketAddr, kind: i32) -> Result<File> {
    let (family, address, len) = Address::encode(addr);
    let file = native::socket_file(family, kind)?;
    // SAFETY: the file owns this socket and address contains len initialized bytes.
    native::status(unsafe { native::bind(file.as_raw_fd(), address.0.as_ptr(), len) })?;
    Ok(file)
}

fn connect(fd: i32, addr: SocketAddr) -> Result<()> {
    let (_, address, len) = Address::encode(addr);
    // SAFETY: the caller keeps the socket live and address contains len initialized bytes.
    native::status(unsafe { native::connect(fd, address.0.as_ptr(), len) })
}

fn socket_addr(fd: i32, peer: bool) -> Result<SocketAddr> {
    let mut address = Address([0; INET6_LEN]);
    let mut len = INET6_LEN as u32;
    // SAFETY: the socket remains live and both output locations are writable.
    let status = unsafe {
        if peer {
            native::getpeername(fd, address.0.as_mut_ptr(), &mut len)
        } else {
            native::getsockname(fd, address.0.as_mut_ptr(), &mut len)
        }
    };
    native::status(status)?;
    address.decode(len)
}

fn receive(fd: i32, buf: &mut [u8], flags: i32) -> Result<usize> {
    // SAFETY: the caller keeps the socket live and buf is writable.
    native::byte_count(unsafe { native::recv(fd, buf.as_mut_ptr(), buf.len(), flags) })
}

fn send(fd: i32, buf: &[u8]) -> Result<usize> {
    // SAFETY: the caller keeps the socket live and buf is readable.
    native::byte_count(unsafe { native::send(fd, buf.as_ptr(), buf.len(), native::NO_SIGNAL) })
}

macro_rules! ip_socket {
    ($name:ident) => {
        impl $name {
            /// Returns the socket's local address.
            pub fn local_addr(&self) -> Result<SocketAddr> {
                socket_addr(self.as_raw_fd(), false)
            }

            /// Selects blocking or nonblocking mode.
            pub fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
                native::set_nonblocking(self.as_raw_fd(), nonblocking)
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
ip_socket!(TcpListener);
ip_socket!(TcpStream);
ip_socket!(UdpSocket);

impl TcpListener {
    /// Binds a listening socket to the first address that succeeds.
    pub fn bind<A: ToSocketAddrs>(addresses: A) -> Result<Self> {
        try_addresses(addresses, |addr| {
            let file = bind_file(addr, STREAM)?;
            // SAFETY: this socket was just bound and the file keeps it live.
            native::status(unsafe { native::listen(file.as_raw_fd(), 128) })?;
            Ok(Self { file })
        })
    }

    /// Accepts a connection and returns the stream and remote address.
    pub fn accept(&self) -> Result<(TcpStream, SocketAddr)> {
        let mut address = Address([0; INET6_LEN]);
        let mut len = INET6_LEN as u32;
        // SAFETY: self keeps the listener live, and address and len are writable.
        let file =
            unsafe { native::accept_file(self.as_raw_fd(), address.0.as_mut_ptr(), &mut len) }?;
        Ok((TcpStream { file }, address.decode(len)?))
    }
}

impl TcpStream {
    /// Connects to the first resolved address that accepts the connection.
    pub fn connect<A: ToSocketAddrs>(addresses: A) -> Result<Self> {
        try_addresses(addresses, |addr| {
            let (family, _, _) = Address::encode(addr);
            let file = native::socket_file(family, STREAM)?;
            connect(file.as_raw_fd(), addr)?;
            Ok(Self { file })
        })
    }

    /// Returns the connected peer's address.
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        socket_addr(self.as_raw_fd(), true)
    }

    /// Reads and clears the socket's pending error.
    pub fn take_error(&self) -> Result<Option<Error>> {
        #[cfg(target_os = "macos")]
        let (level, option) = (0xffff, 0x1007);
        #[cfg(target_os = "linux")]
        let (level, option) = (1, 4);
        let mut code = 0i32;
        let mut len = core::mem::size_of::<i32>() as u32;
        // SAFETY: SO_ERROR writes a native int into code. Both outputs and the socket are live.
        native::status(unsafe {
            native::getsockopt(
                self.as_raw_fd(),
                level,
                option,
                (&mut code as *mut i32).cast(),
                &mut len,
            )
        })?;
        Ok((code != 0).then(|| Error::from_raw_os_error(code)))
    }

    /// Reads bytes without advancing the receive queue.
    pub fn peek(&self, buf: &mut [u8]) -> Result<usize> {
        receive(self.as_raw_fd(), buf, native::PEEK)
    }
}

impl Read for &TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        receive(self.as_raw_fd(), buf, 0)
    }
}
impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        (&*self).read(buf)
    }
}
impl Write for &TcpStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        send(self.as_raw_fd(), buf)
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        (&*self).write(buf)
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl UdpSocket {
    /// Binds a UDP socket to the first address that succeeds.
    pub fn bind<A: ToSocketAddrs>(addresses: A) -> Result<Self> {
        try_addresses(addresses, |addr| {
            let file = bind_file(addr, DATAGRAM)?;
            Ok(Self { file })
        })
    }

    /// Selects the remote peer for subsequent sends and receives.
    pub fn connect<A: ToSocketAddrs>(&self, addresses: A) -> Result<()> {
        try_addresses(addresses, |addr| connect(self.as_raw_fd(), addr))
    }

    /// Returns the connected peer's address.
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        socket_addr(self.as_raw_fd(), true)
    }

    /// Sends a datagram to the first resolved address.
    pub fn send_to<A: ToSocketAddrs>(&self, buf: &[u8], addresses: A) -> Result<usize> {
        let addr = addresses
            .to_socket_addrs()?
            .next()
            .ok_or_else(no_addresses)?;
        let (_, address, len) = Address::encode(addr);
        // SAFETY: the owned socket and both input buffers are live and readable.
        native::byte_count(unsafe {
            native::sendto(
                self.as_raw_fd(),
                buf.as_ptr(),
                buf.len(),
                native::NO_SIGNAL,
                address.0.as_ptr(),
                len,
            )
        })
    }

    fn receive_from(&self, buf: &mut [u8], flags: i32) -> Result<(usize, SocketAddr)> {
        let mut address = Address([0; INET6_LEN]);
        let mut len = INET6_LEN as u32;
        // SAFETY: the socket stays live and all output buffers are writable.
        let count = native::byte_count(unsafe {
            native::recvfrom(
                self.as_raw_fd(),
                buf.as_mut_ptr(),
                buf.len(),
                flags,
                address.0.as_mut_ptr(),
                &mut len,
            )
        })?;
        Ok((count, address.decode(len)?))
    }

    /// Receives a datagram with its sender's address.
    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.receive_from(buf, 0)
    }

    /// Reads a datagram and its sender's address without consuming it.
    pub fn peek_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.receive_from(buf, native::PEEK)
    }

    /// Receives a datagram from the connected peer.
    pub fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        receive(self.as_raw_fd(), buf, 0)
    }

    /// Reads a datagram from the connected peer without consuming it.
    pub fn peek(&self, buf: &mut [u8]) -> Result<usize> {
        receive(self.as_raw_fd(), buf, native::PEEK)
    }

    /// Sends a datagram to the connected peer.
    pub fn send(&self, buf: &[u8]) -> Result<usize> {
        send(self.as_raw_fd(), buf)
    }
}
