//! IP and socket-address values and host-name resolution.
use crate::alloc_crate::string::String;
use core::fmt::{self, Write};
use core::hash::{Hash, Hasher};
use core::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ipv4Addr {
    octets: [u8; 4],
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ipv6Addr {
    octets: [u8; 16],
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IpAddr {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SocketAddrV4 {
    ip: Ipv4Addr,
    port: u16,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SocketAddrV6 {
    ip: Ipv6Addr,
    port: u16,
    flowinfo: u32,
    scope_id: u32,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SocketAddr {
    V4(SocketAddrV4),
    V6(SocketAddrV6),
}

impl Ipv4Addr {
    pub const LOCALHOST: Self = Self::new(127, 0, 0, 1);
    pub const UNSPECIFIED: Self = Self::new(0, 0, 0, 0);
    pub const BROADCAST: Self = Self::new(255, 255, 255, 255);
    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self {
            octets: [a, b, c, d],
        }
    }
    pub const fn octets(&self) -> [u8; 4] {
        self.octets
    }
    pub const fn to_bits(self) -> u32 {
        u32::from_be_bytes(self.octets)
    }
    pub const fn from_bits(bits: u32) -> Self {
        Self {
            octets: bits.to_be_bytes(),
        }
    }
    pub const fn is_unspecified(&self) -> bool {
        self.to_bits() == 0
    }
    pub const fn is_loopback(&self) -> bool {
        self.octets[0] == 127
    }
    pub const fn is_multicast(&self) -> bool {
        self.octets[0] >= 224 && self.octets[0] <= 239
    }
    pub const fn is_private(&self) -> bool {
        self.octets[0] == 10
            || self.octets[0] == 172 && self.octets[1] >= 16 && self.octets[1] <= 31
            || self.octets[0] == 192 && self.octets[1] == 168
    }
    pub const fn is_link_local(&self) -> bool {
        self.octets[0] == 169 && self.octets[1] == 254
    }
    pub const fn is_broadcast(&self) -> bool {
        self.to_bits() == u32::MAX
    }
    pub const fn to_ipv6_mapped(&self) -> Ipv6Addr {
        Ipv6Addr::new(
            0,
            0,
            0,
            0,
            0,
            0xffff,
            ((self.octets[0] as u16) << 8) | self.octets[1] as u16,
            ((self.octets[2] as u16) << 8) | self.octets[3] as u16,
        )
    }
    pub const fn to_ipv6_compatible(&self) -> Ipv6Addr {
        Ipv6Addr::new(
            0,
            0,
            0,
            0,
            0,
            0,
            ((self.octets[0] as u16) << 8) | self.octets[1] as u16,
            ((self.octets[2] as u16) << 8) | self.octets[3] as u16,
        )
    }
}
impl Ipv6Addr {
    pub const LOCALHOST: Self = Self::new(0, 0, 0, 0, 0, 0, 0, 1);
    pub const UNSPECIFIED: Self = Self::new(0, 0, 0, 0, 0, 0, 0, 0);
    #[expect(
        clippy::too_many_arguments,
        reason = "the standard IPv6 constructor takes eight segments"
    )]
    pub const fn new(a: u16, b: u16, c: u16, d: u16, e: u16, f: u16, g: u16, h: u16) -> Self {
        Self {
            octets: [
                (a >> 8) as u8,
                a as u8,
                (b >> 8) as u8,
                b as u8,
                (c >> 8) as u8,
                c as u8,
                (d >> 8) as u8,
                d as u8,
                (e >> 8) as u8,
                e as u8,
                (f >> 8) as u8,
                f as u8,
                (g >> 8) as u8,
                g as u8,
                (h >> 8) as u8,
                h as u8,
            ],
        }
    }
    pub const fn octets(&self) -> [u8; 16] {
        self.octets
    }
    pub const fn segments(&self) -> [u16; 8] {
        let b = self.octets;
        [
            u16::from_be_bytes([b[0], b[1]]),
            u16::from_be_bytes([b[2], b[3]]),
            u16::from_be_bytes([b[4], b[5]]),
            u16::from_be_bytes([b[6], b[7]]),
            u16::from_be_bytes([b[8], b[9]]),
            u16::from_be_bytes([b[10], b[11]]),
            u16::from_be_bytes([b[12], b[13]]),
            u16::from_be_bytes([b[14], b[15]]),
        ]
    }
    pub const fn to_bits(self) -> u128 {
        u128::from_be_bytes(self.octets)
    }
    pub const fn from_bits(bits: u128) -> Self {
        Self {
            octets: bits.to_be_bytes(),
        }
    }
    pub const fn is_unspecified(&self) -> bool {
        self.to_bits() == 0
    }
    pub const fn is_loopback(&self) -> bool {
        self.to_bits() == 1
    }
    pub const fn is_multicast(&self) -> bool {
        self.octets[0] == 0xff
    }
    pub const fn to_ipv4_mapped(&self) -> Option<Ipv4Addr> {
        let s = self.segments();
        if s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0xffff {
            Some(Ipv4Addr::new(
                self.octets[12],
                self.octets[13],
                self.octets[14],
                self.octets[15],
            ))
        } else {
            None
        }
    }
    pub const fn to_ipv4(&self) -> Option<Ipv4Addr> {
        let s = self.segments();
        if s[0] == 0
            && s[1] == 0
            && s[2] == 0
            && s[3] == 0
            && s[4] == 0
            && (s[5] == 0 || s[5] == 0xffff)
        {
            Some(Ipv4Addr::new(
                self.octets[12],
                self.octets[13],
                self.octets[14],
                self.octets[15],
            ))
        } else {
            None
        }
    }
}
impl IpAddr {
    pub const fn is_ipv4(&self) -> bool {
        matches!(self, Self::V4(_))
    }
    pub const fn is_ipv6(&self) -> bool {
        matches!(self, Self::V6(_))
    }
    pub const fn is_unspecified(&self) -> bool {
        match self {
            Self::V4(ip) => ip.is_unspecified(),
            Self::V6(ip) => ip.is_unspecified(),
        }
    }
    pub const fn is_loopback(&self) -> bool {
        match self {
            Self::V4(ip) => ip.is_loopback(),
            Self::V6(ip) => ip.is_loopback(),
        }
    }
    pub const fn is_multicast(&self) -> bool {
        match self {
            Self::V4(ip) => ip.is_multicast(),
            Self::V6(ip) => ip.is_multicast(),
        }
    }
    pub const fn to_canonical(&self) -> Self {
        match self {
            Self::V6(ip) => match ip.to_ipv4_mapped() {
                Some(v4) => Self::V4(v4),
                None => *self,
            },
            _ => *self,
        }
    }
}
impl SocketAddrV4 {
    pub const fn new(ip: Ipv4Addr, port: u16) -> Self {
        Self { ip, port }
    }
    pub const fn ip(&self) -> &Ipv4Addr {
        &self.ip
    }
    pub const fn port(&self) -> u16 {
        self.port
    }
    pub fn set_ip(&mut self, ip: Ipv4Addr) {
        self.ip = ip;
    }
    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }
}
impl SocketAddrV6 {
    pub const fn new(ip: Ipv6Addr, port: u16, flowinfo: u32, scope_id: u32) -> Self {
        Self {
            ip,
            port,
            flowinfo,
            scope_id,
        }
    }
    pub const fn ip(&self) -> &Ipv6Addr {
        &self.ip
    }
    pub const fn port(&self) -> u16 {
        self.port
    }
    pub const fn flowinfo(&self) -> u32 {
        self.flowinfo
    }
    pub const fn scope_id(&self) -> u32 {
        self.scope_id
    }
    pub fn set_ip(&mut self, ip: Ipv6Addr) {
        self.ip = ip;
    }
    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }
    pub fn set_flowinfo(&mut self, value: u32) {
        self.flowinfo = value;
    }
    pub fn set_scope_id(&mut self, value: u32) {
        self.scope_id = value;
    }
}
impl SocketAddr {
    pub const fn new(ip: IpAddr, port: u16) -> Self {
        match ip {
            IpAddr::V4(ip) => Self::V4(SocketAddrV4::new(ip, port)),
            IpAddr::V6(ip) => Self::V6(SocketAddrV6::new(ip, port, 0, 0)),
        }
    }
    pub const fn ip(&self) -> IpAddr {
        match self {
            Self::V4(a) => IpAddr::V4(*a.ip()),
            Self::V6(a) => IpAddr::V6(*a.ip()),
        }
    }
    pub const fn port(&self) -> u16 {
        match self {
            Self::V4(a) => a.port(),
            Self::V6(a) => a.port(),
        }
    }
    pub const fn is_ipv4(&self) -> bool {
        matches!(self, Self::V4(_))
    }
    pub const fn is_ipv6(&self) -> bool {
        matches!(self, Self::V6(_))
    }
    pub fn set_port(&mut self, port: u16) {
        match self {
            Self::V4(a) => a.set_port(port),
            Self::V6(a) => a.set_port(port),
        }
    }
    pub fn set_ip(&mut self, ip: IpAddr) {
        match (self, ip) {
            (Self::V4(a), IpAddr::V4(ip)) => a.set_ip(ip),
            (Self::V6(a), IpAddr::V6(ip)) => a.set_ip(ip),
            (a, ip) => {
                let port = a.port();
                *a = Self::new(ip, port);
            }
        }
    }
}
impl From<[u8; 4]> for Ipv4Addr {
    fn from(octets: [u8; 4]) -> Self {
        Self { octets }
    }
}
impl From<u32> for Ipv4Addr {
    fn from(value: u32) -> Self {
        Self::from_bits(value)
    }
}
impl From<Ipv4Addr> for u32 {
    fn from(value: Ipv4Addr) -> Self {
        value.to_bits()
    }
}
impl From<[u8; 16]> for Ipv6Addr {
    fn from(octets: [u8; 16]) -> Self {
        Self { octets }
    }
}
impl From<[u16; 8]> for Ipv6Addr {
    fn from(s: [u16; 8]) -> Self {
        Self::new(s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7])
    }
}
impl From<u128> for Ipv6Addr {
    fn from(value: u128) -> Self {
        Self::from_bits(value)
    }
}
impl From<Ipv6Addr> for u128 {
    fn from(value: Ipv6Addr) -> Self {
        value.to_bits()
    }
}
impl From<Ipv4Addr> for IpAddr {
    fn from(value: Ipv4Addr) -> Self {
        Self::V4(value)
    }
}
impl From<Ipv6Addr> for IpAddr {
    fn from(value: Ipv6Addr) -> Self {
        Self::V6(value)
    }
}
impl From<[u8; 4]> for IpAddr {
    fn from(value: [u8; 4]) -> Self {
        Self::V4(Ipv4Addr::from(value))
    }
}
impl From<[u8; 16]> for IpAddr {
    fn from(value: [u8; 16]) -> Self {
        Self::V6(Ipv6Addr::from(value))
    }
}
impl From<[u16; 8]> for IpAddr {
    fn from(value: [u16; 8]) -> Self {
        Self::V6(Ipv6Addr::from(value))
    }
}
impl From<SocketAddrV4> for SocketAddr {
    fn from(value: SocketAddrV4) -> Self {
        Self::V4(value)
    }
}
impl From<SocketAddrV6> for SocketAddr {
    fn from(value: SocketAddrV6) -> Self {
        Self::V6(value)
    }
}
impl<I: Into<IpAddr>> From<(I, u16)> for SocketAddr {
    fn from((ip, port): (I, u16)) -> Self {
        Self::new(ip.into(), port)
    }
}
impl Hash for Ipv4Addr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        u32::from_ne_bytes(self.octets).hash(state);
    }
}
impl Hash for Ipv6Addr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        u128::from_ne_bytes(self.octets).hash(state);
    }
}

impl fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&crate::format!(
            "{}.{}.{}.{}",
            self.octets[0],
            self.octets[1],
            self.octets[2],
            self.octets[3]
        ))
    }
}
impl fmt::Display for Ipv6Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ip) = self.to_ipv4_mapped() {
            return f.pad(&crate::format!("::ffff:{}", ip));
        }
        let segments = self.segments();
        let mut start = 0;
        let mut best_start = 0;
        let mut best_len = 0;
        while start < 8 {
            if segments[start] != 0 {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < 8 && segments[end] == 0 {
                end += 1;
            }
            if end - start > best_len {
                best_start = start;
                best_len = end - start;
            }
            start = end;
        }
        let mut result = String::new();
        let mut i = 0;
        while i < 8 {
            if best_len > 1 && i == best_start {
                result.push_str("::");
                i += best_len;
                if i == 8 {
                    break;
                }
            } else {
                if !result.is_empty() && !result.ends_with(':') {
                    result.push(':');
                }
                write!(&mut result, "{:x}", segments[i])?;
                i += 1;
            }
        }
        f.pad(&result)
    }
}
impl fmt::Display for IpAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V4(ip) => fmt::Display::fmt(ip, f),
            Self::V6(ip) => fmt::Display::fmt(ip, f),
        }
    }
}
impl fmt::Display for SocketAddrV4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&crate::format!("{}:{}", self.ip, self.port))
    }
}
impl fmt::Display for SocketAddrV6 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scope_id == 0 {
            f.pad(&crate::format!("[{}]:{}", self.ip, self.port))
        } else {
            f.pad(&crate::format!(
                "[{}%{}]:{}",
                self.ip,
                self.scope_id,
                self.port
            ))
        }
    }
}
impl fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V4(a) => fmt::Display::fmt(a, f),
            Self::V6(a) => fmt::Display::fmt(a, f),
        }
    }
}
macro_rules! display_debug {($($t:ty),*)=>{$(impl fmt::Debug for $t{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{fmt::Display::fmt(self,f)}})*};}
display_debug!(
    Ipv4Addr,
    Ipv6Addr,
    IpAddr,
    SocketAddrV4,
    SocketAddrV6,
    SocketAddr
);

#[derive(Clone, PartialEq, Eq, Debug)]
enum AddrKind {
    Ip,
    Ipv4,
    Ipv6,
    Socket,
    SocketV4,
    SocketV6,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AddrParseError(AddrKind);
impl fmt::Display for AddrParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self.0 {
            AddrKind::Ip => "invalid IP address syntax",
            AddrKind::Ipv4 => "invalid IPv4 address syntax",
            AddrKind::Ipv6 => "invalid IPv6 address syntax",
            AddrKind::Socket => "invalid socket address syntax",
            AddrKind::SocketV4 => "invalid IPv4 socket address syntax",
            AddrKind::SocketV6 => "invalid IPv6 socket address syntax",
        })
    }
}
impl core::error::Error for AddrParseError {}

fn decimal(bytes: &[u8], max: u32) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for byte in bytes {
        if *byte < b'0' || *byte > b'9' {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((*byte - b'0') as u32)?;
        if value > max {
            return None;
        }
    }
    Some(value)
}
fn ipv4(text: &str) -> Option<Ipv4Addr> {
    let bytes = text.as_bytes();
    let mut parts = [0; 4];
    let mut cursor = 0;
    let mut index = 0;
    while index < 4 {
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b'.' {
            cursor += 1;
        }
        let part = &bytes[start..cursor];
        if part.len() > 3 || part.len() > 1 && part[0] == b'0' {
            return None;
        }
        parts[index] = decimal(part, 255)? as u8;
        index += 1;
        if index < 4 {
            if cursor == bytes.len() {
                return None;
            }
            cursor += 1;
        }
    }
    if cursor != bytes.len() {
        None
    } else {
        Some(Ipv4Addr::from(parts))
    }
}
fn ipv6(text: &str) -> Option<Ipv6Addr> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut groups = [0u16; 8];
    let mut count = 0;
    let mut cursor = 0;
    let mut compression = None;
    if bytes[0] == b':' {
        if bytes.get(1) != Some(&b':') {
            return None;
        }
        compression = Some(0);
        cursor = 2;
    }
    while cursor < bytes.len() {
        if count == 8 {
            return None;
        }
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b':' {
            cursor += 1;
        }
        let group = &bytes[start..cursor];
        if group.contains(&b'.') {
            if count > 6 || cursor != bytes.len() {
                return None;
            }
            let v4 = ipv4(&text[start..])?.octets();
            groups[count] = u16::from_be_bytes([v4[0], v4[1]]);
            groups[count + 1] = u16::from_be_bytes([v4[2], v4[3]]);
            count += 2;
            break;
        }
        if group.is_empty() || group.len() > 4 {
            return None;
        }
        let mut value = 0u16;
        for byte in group {
            let digit = match byte {
                b'0'..=b'9' => *byte - b'0',
                b'a'..=b'f' => *byte - b'a' + 10,
                b'A'..=b'F' => *byte - b'A' + 10,
                _ => return None,
            };
            value = (value << 4) | digit as u16;
        }
        groups[count] = value;
        count += 1;
        if cursor == bytes.len() {
            break;
        }
        cursor += 1;
        if cursor < bytes.len() && bytes[cursor] == b':' {
            if compression.is_some() {
                return None;
            }
            compression = Some(count);
            cursor += 1;
        } else if cursor == bytes.len() {
            return None;
        }
    }
    if let Some(at) = compression {
        if count == 8 {
            return None;
        }
        let tail = count - at;
        let mut i = tail;
        while i > 0 {
            i -= 1;
            groups[8 - tail + i] = groups[at + i];
        }
        let mut i = at;
        while i < 8 - tail {
            groups[i] = 0;
            i += 1;
        }
    } else if count != 8 {
        return None;
    }
    Some(Ipv6Addr::from(groups))
}
impl FromStr for Ipv4Addr {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ipv4(s).ok_or(AddrParseError(AddrKind::Ipv4))
    }
}
impl FromStr for Ipv6Addr {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ipv6(s).ok_or(AddrParseError(AddrKind::Ipv6))
    }
}
impl FromStr for IpAddr {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(ip) = ipv4(s) {
            Ok(Self::V4(ip))
        } else {
            ipv6(s).map(Self::V6).ok_or(AddrParseError(AddrKind::Ip))
        }
    }
}
impl FromStr for SocketAddrV4 {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split = s.find(':').ok_or(AddrParseError(AddrKind::SocketV4))?;
        let ip = ipv4(&s[..split]).ok_or(AddrParseError(AddrKind::SocketV4))?;
        let port = decimal(&s.as_bytes()[split + 1..], u16::MAX as u32)
            .ok_or(AddrParseError(AddrKind::SocketV4))?;
        Ok(Self::new(ip, port as u16))
    }
}
impl FromStr for SocketAddrV6 {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let error = || AddrParseError(AddrKind::SocketV6);
        if !s.starts_with('[') {
            return Err(error());
        }
        let close = s.find(']').ok_or_else(error)?;
        let inside = &s[1..close];
        let (host, scope) = if let Some(split) = inside.find('%') {
            (
                &inside[..split],
                decimal(&inside.as_bytes()[split + 1..], u32::MAX).ok_or_else(error)?,
            )
        } else {
            (inside, 0)
        };
        let ip = ipv6(host).ok_or_else(error)?;
        if s.as_bytes().get(close + 1) != Some(&b':') {
            return Err(error());
        }
        let port = decimal(&s.as_bytes()[close + 2..], u16::MAX as u32).ok_or_else(error)?;
        Ok(Self::new(ip, port as u16, 0, scope))
    }
}
impl FromStr for SocketAddr {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(a) = s.parse::<SocketAddrV4>() {
            Ok(Self::V4(a))
        } else {
            s.parse::<SocketAddrV6>()
                .map(Self::V6)
                .map_err(|_| AddrParseError(AddrKind::Socket))
        }
    }
}

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
