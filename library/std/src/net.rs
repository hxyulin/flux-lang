//! Network address types, host-name resolution and socket ownership.
pub use core::net::{AddrParseError, IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

/// Which halves of a socket connection to shut down.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Shutdown {
    Read,
    Write,
    Both,
}

#[path = "net/lookup.rs"]
mod lookup;
pub use lookup::ToSocketAddrs;

// Socket ownership can be transferred to and from native socket libraries.
// File supplies the existing descriptor lifetime and duplication.
macro_rules! socket_owner {
    ($name:ident) => {
        pub struct $name {
            file: crate::fs::File,
        }
        impl $name {
            pub fn try_clone(&self) -> crate::io::Result<Self> {
                Ok(Self {
                    file: self.file.try_clone()?,
                })
            }
        }
        impl crate::os::fd::AsRawFd for $name {
            fn as_raw_fd(&self) -> i32 {
                crate::os::fd::AsRawFd::as_raw_fd(&self.file)
            }
        }
        impl crate::os::fd::IntoRawFd for $name {
            fn into_raw_fd(self) -> i32 {
                crate::os::fd::IntoRawFd::into_raw_fd(self.file)
            }
        }
        impl crate::os::fd::FromRawFd for $name {
            /// # Safety
            /// fd must be a valid, uniquely owned descriptor of this socket kind.
            unsafe fn from_raw_fd(fd: i32) -> Self {
                Self {
                    // SAFETY: the caller transfers exclusive ownership of this descriptor.
                    file: unsafe { crate::os::fd::FromRawFd::from_raw_fd(fd) },
                }
            }
        }
        impl crate::os::fd::AsFd for $name {
            fn as_fd(&self) -> crate::os::fd::BorrowedFd<'_> {
                crate::os::fd::AsFd::as_fd(&self.file)
            }
        }
        impl From<crate::os::fd::OwnedFd> for $name {
            fn from(fd: crate::os::fd::OwnedFd) -> Self {
                Self { file: fd.into() }
            }
        }
        impl From<$name> for crate::os::fd::OwnedFd {
            fn from(socket: $name) -> Self {
                socket.file.into()
            }
        }
    };
}
socket_owner!(TcpStream);
socket_owner!(TcpListener);
socket_owner!(UdpSocket);

#[path = "net/unix.rs"]
pub(crate) mod unix;
