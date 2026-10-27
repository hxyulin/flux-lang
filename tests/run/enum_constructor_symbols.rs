use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6};
fn main() {
    let make_ip: fn(Ipv6Addr) -> IpAddr = IpAddr::V6;
    let make_socket: fn(SocketAddrV6) -> SocketAddr = SocketAddr::V6;
    let ip = make_ip(Ipv6Addr::LOCALHOST);
    let socket = make_socket(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8080, 17, 42));
    assert_eq!(ip.to_string(), "::1");
    assert_eq!(socket.to_string(), "[::1%42]:8080");
    assert_eq!(Some(Ipv6Addr::LOCALHOST).map(IpAddr::V6), Some(ip));
    assert_eq!(
        Some(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8080, 17, 42)).map(SocketAddr::V6),
        Some(socket)
    );
    let option: fn(u32) -> Option<u32> = Option::Some;
    assert_eq!(option(7), Some(7));
    println!("constructors {} {} {}", ip, socket, option(7).unwrap());
}
