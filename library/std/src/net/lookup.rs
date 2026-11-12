//! Socket-address conversion and platform host lookup.
use super::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use crate::alloc_crate::{string::String, vec::Vec};
use crate::io;

/// Converts a value into socket addresses, resolving host names when necessary.
pub trait ToSocketAddrs {
    type Iter: Iterator<Item = SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter>;
}
impl ToSocketAddrs for SocketAddr {
    type Iter = core::option::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        Ok(Some(*self).into_iter())
    }
}
impl ToSocketAddrs for SocketAddrV4 {
    type Iter = core::option::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        SocketAddr::V4(*self).to_socket_addrs()
    }
}
impl ToSocketAddrs for SocketAddrV6 {
    type Iter = core::option::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        SocketAddr::V6(*self).to_socket_addrs()
    }
}
impl ToSocketAddrs for (IpAddr, u16) {
    type Iter = core::option::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        SocketAddr::new(self.0, self.1).to_socket_addrs()
    }
}
impl ToSocketAddrs for (Ipv4Addr, u16) {
    type Iter = core::option::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        (IpAddr::V4(self.0), self.1).to_socket_addrs()
    }
}
impl ToSocketAddrs for (Ipv6Addr, u16) {
    type Iter = core::option::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        (IpAddr::V6(self.0), self.1).to_socket_addrs()
    }
}
impl ToSocketAddrs for (&str, u16) {
    type Iter = crate::alloc_crate::vec::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        if let Ok(ip) = self.0.parse::<IpAddr>() {
            let addresses = crate::alloc_crate::vec![SocketAddr::new(ip, self.1)];
            return Ok(addresses.into_iter());
        }
        lookup(self.0, self.1).map(Vec::into_iter)
    }
}
impl ToSocketAddrs for (String, u16) {
    type Iter = crate::alloc_crate::vec::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        (self.0.as_str(), self.1).to_socket_addrs()
    }
}
impl ToSocketAddrs for str {
    type Iter = crate::alloc_crate::vec::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        if let Ok(address) = self.parse::<SocketAddr>() {
            let addresses = crate::alloc_crate::vec![address];
            return Ok(addresses.into_iter());
        }
        let (host, port) = self
            .rsplit_once(':')
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid socket address"))?;
        let port = port
            .parse::<u16>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid port value"))?;
        (host, port).to_socket_addrs()
    }
}
impl ToSocketAddrs for String {
    type Iter = crate::alloc_crate::vec::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        self.as_str().to_socket_addrs()
    }
}
impl<'a> ToSocketAddrs for &'a [SocketAddr] {
    type Iter = core::iter::Copied<core::slice::Iter<'a, SocketAddr>>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        Ok(self.iter().copied())
    }
}
impl<T: ToSocketAddrs + ?Sized> ToSocketAddrs for &T {
    type Iter = T::Iter;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        (**self).to_socket_addrs()
    }
}

#[repr(C)]
struct AddrInfo {
    flags: i32,
    family: i32,
    socktype: i32,
    protocol: i32,
    addrlen: u32,
    #[cfg(target_os = "macos")]
    canonname: *mut u8,
    addr: *mut u8,
    #[cfg(not(target_os = "macos"))]
    canonname: *mut u8,
    next: *mut AddrInfo,
}
unsafe extern "C" {
    fn getaddrinfo(
        host: *const u8,
        service: *const u8,
        hints: *const AddrInfo,
        result: *mut *mut AddrInfo,
    ) -> i32;
    fn freeaddrinfo(result: *mut AddrInfo);
    fn gai_strerror(error: i32) -> *const core::ffi::c_char;
}
struct Lookup(*mut AddrInfo);
impl Drop for Lookup {
    fn drop(&mut self) {
        // SAFETY: this is the unique owner of getaddrinfo's successful result.
        unsafe { freeaddrinfo(self.0) }
    }
}
fn lookup(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    use crate::ffi::{CStr, CString};
    let host = CString::new(host).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "host name contains a nul byte")
    })?;
    let hints = AddrInfo {
        flags: 0,
        family: 0,
        socktype: 1,
        protocol: 0,
        addrlen: 0,
        canonname: core::ptr::null_mut(),
        addr: core::ptr::null_mut(),
        next: core::ptr::null_mut(),
    };
    let mut result = core::ptr::null_mut();
    // SAFETY: host is nul-terminated; the hints and output live for the call.
    let status = unsafe {
        getaddrinfo(
            host.as_ptr() as *const u8,
            core::ptr::null(),
            &hints,
            &mut result,
        )
    };
    if status != 0 {
        #[cfg(target_os = "macos")]
        let system_error = 11;
        #[cfg(not(target_os = "macos"))]
        let system_error = -11;
        if status == system_error {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: gai_strerror returns a static nul-terminated error message.
        let message = unsafe { CStr::from_ptr(gai_strerror(status)) }
            .to_string_lossy()
            .into_owned();
        return Err(io::Error::other(message));
    }
    let owner = Lookup(result);
    let mut addresses = Vec::new();
    let mut current = owner.0;
    while !current.is_null() {
        // SAFETY: the list and each address remain owned by `owner` until return.
        let info = unsafe { &*current };
        #[cfg(target_os = "macos")]
        let ipv6 = 30;
        #[cfg(not(target_os = "macos"))]
        let ipv6 = 10;
        if !info.addr.is_null() && info.family == 2 && info.addrlen >= 16 {
            // SAFETY: the IPv4 sockaddr has at least 16 bytes; its address is bytes 4..8.
            let bytes = unsafe { core::slice::from_raw_parts(info.addr, 16) };
            addresses.push(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(bytes[4], bytes[5], bytes[6], bytes[7])),
                port,
            ));
        } else if !info.addr.is_null() && info.family == ipv6 && info.addrlen >= 28 {
            // SAFETY: the IPv6 sockaddr has at least 28 bytes with the platform's standard layout.
            let bytes = unsafe { core::slice::from_raw_parts(info.addr, 28) };
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&bytes[8..24]);
            let flowinfo = u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            let scope = u32::from_ne_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
            addresses.push(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(octets),
                port,
                flowinfo,
                scope,
            )));
        }
        current = info.next;
    }
    Ok(addresses)
}
