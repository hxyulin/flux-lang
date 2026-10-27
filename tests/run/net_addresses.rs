use std::fmt::{Debug, Display};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::str::FromStr;

fn check<T: FromStr + Display + Debug>(text: &str) -> String
where
    T::Err: Display + Debug,
{
    match text.parse::<T>() {
        Ok(value) => format!("{} {:?} {:*>48.13}", value, value, value),
        Err(error) => format!("{} {:?}", error, error),
    }
}
fn digest(hash: &mut u64, text: &str) {
    for byte in text.bytes() {
        *hash = (*hash ^ byte as u64).wrapping_mul(0x100000001b3);
    }
}
fn all(hash: &mut u64, text: &str) {
    digest(hash, &check::<IpAddr>(text));
    digest(hash, &check::<Ipv4Addr>(text));
    digest(hash, &check::<Ipv6Addr>(text));
    digest(hash, &check::<SocketAddr>(text));
    digest(hash, &check::<SocketAddrV4>(text));
    digest(hash, &check::<SocketAddrV6>(text));
}
const LOCAL: Ipv6Addr = Ipv6Addr::LOCALHOST;
const V4: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 1);
const MAPPED: Ipv6Addr = V4.to_ipv6_mapped();
fn main() {
    assert_eq!(
        LOCAL.octets(),
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
    );
    assert_eq!(MAPPED.to_string(), "::ffff:192.0.2.1");
    assert_eq!(MAPPED.to_ipv4_mapped(), Some(V4));
    assert_eq!(IpAddr::V6(MAPPED).to_canonical(), IpAddr::V4(V4));
    assert_eq!(Ipv4Addr::from(u32::from(V4)), V4);
    assert_eq!(Ipv6Addr::from(u128::from(MAPPED)), MAPPED);
    assert_eq!(Ipv6Addr::from(MAPPED.segments()), MAPPED);
    assert_eq!(Ipv6Addr::from(MAPPED.octets()), MAPPED);
    let mut address: SocketAddr = (V4, 4312).into();
    assert!(address.is_ipv4());
    assert_eq!(address.ip(), IpAddr::V4(V4));
    address.set_ip(IpAddr::V6(LOCAL));
    assert!(address.is_ipv6());
    assert_eq!(address.port(), 4312);
    address.set_port(65535);
    assert_eq!(address.to_string(), "[::1]:65535");
    let mut scoped = SocketAddrV6::new(LOCAL, 8080, u32::MAX, u32::MAX);
    assert_eq!(scoped.flowinfo(), u32::MAX);
    assert_eq!(scoped.scope_id(), u32::MAX);
    assert_eq!(scoped.to_string(), "[::1%4294967295]:8080");
    scoped.set_ip(MAPPED);
    scoped.set_flowinfo(12);
    scoped.set_scope_id(7);
    scoped.set_port(0);
    assert_eq!(scoped.to_string(), "[::ffff:192.0.2.1%7]:0");
    assert_eq!(scoped.flowinfo(), 12);
    let mut socket4 = SocketAddrV4::new(V4, 12);
    socket4.set_ip(Ipv4Addr::LOCALHOST);
    socket4.set_port(13);
    assert_eq!(socket4.to_string(), "127.0.0.1:13");
    let mut hash = 0xcbf29ce484222325;
    for text in [
        "",
        "0.0.0.0",
        "127.0.0.1",
        "255.255.255.255",
        "01.2.3.4",
        "1.02.3.4",
        "1.2.3.4.",
        "1.2.3",
        "256.0.0.1",
        "-1.2.3.4",
        "+1.2.3.4",
        "1.2.3.4 ",
        "::",
        "::1",
        "1::",
        "::ffff:192.0.2.1",
        "::192.0.2.1",
        "1:2:3:4:5:6:192.0.2.1",
        "1:2:3:4:5:6:7:8",
        "1:2:3:4:5:6:7:8::",
        "1::2::3",
        ":::1",
        "1:::2",
        "[::1]",
        "[::1]:80",
        "[::1%42]:80",
        "[::1%4294967295]:65535",
        "[::1%4294967296]:0",
        "[::1%eth0]:1",
        "[::1%]:1",
        "[::1]:65536",
        "[::1]:+80",
        "[::1]:-1",
        "[::1]:000000000080",
        "127.0.0.1:000000000080",
        "[127.0.0.1]:80",
        "🦀",
    ] {
        all(&mut hash, text);
    }
    for mask in 0..256u16 {
        let segments: [u16; 8] = core::array::from_fn(|i| {
            if mask & (1 << i) != 0 {
                0
            } else {
                i as u16 + 1
            }
        });
        let ip = Ipv6Addr::from(segments);
        all(&mut hash, &ip.to_string());
        all(
            &mut hash,
            &format!("[{}%{}]:{}", ip, mask as u32 * 1024, mask),
        );
    }
    let mut random = 0x9c27_38ff_1120_dd29u64;
    for _ in 0..256 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let v4 = Ipv4Addr::from(random as u32);
        all(&mut hash, &v4.to_string());
        all(&mut hash, &format!("{}:{}", v4, random as u16));
        let v6 = Ipv6Addr::from((random as u128) << 64 | random.rotate_left(29) as u128);
        all(&mut hash, &v6.to_string());
        all(
            &mut hash,
            &format!("[{}%{}]:{}", v6, random as u32, random as u16),
        );
    }
    println!("addresses {}", hash);
}
