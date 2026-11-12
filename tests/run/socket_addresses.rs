use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
fn main() {
    println!("{:?}", "127.0.0.1:80".to_socket_addrs().unwrap().collect::<Vec<_>>());
    println!("{:?}", "[::1]:81".to_socket_addrs().unwrap().collect::<Vec<_>>());
    println!("{:?}", ("127.0.0.1",82).to_socket_addrs().unwrap().collect::<Vec<_>>());
    println!("{:?}", (String::from("::1"),83).to_socket_addrs().unwrap().collect::<Vec<_>>());
    println!("{:?}", (Ipv4Addr::LOCALHOST,84).to_socket_addrs().unwrap().collect::<Vec<_>>());
    println!("{:?}", (Ipv6Addr::LOCALHOST,85).to_socket_addrs().unwrap().collect::<Vec<_>>());
    let addresses=[SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST),86)];
    println!("{:?}", addresses.as_slice().to_socket_addrs().unwrap().collect::<Vec<_>>());
    let local=("localhost",87).to_socket_addrs().unwrap().collect::<Vec<_>>();
    println!("{} {}",!local.is_empty(),local.iter().all(|a|a.ip().is_loopback()&&a.port()==87));
    println!("{} {} {}",("bad\0host",88).to_socket_addrs().is_err(),"localhost:65536".to_socket_addrs().is_err(),"localhost".to_socket_addrs().is_err());
    let mut text=String::from("text");text.try_reserve(17).unwrap();text.try_reserve_exact(3).unwrap();
    println!("{} {}",text.try_reserve(usize::MAX).is_err(),text);
    use std::path::Prefix;
    println!("{} {} {}",Prefix::Disk(b'C').is_verbatim(),Prefix::VerbatimDisk(b'C').is_verbatim(),std::path::Path::new("C:\\dir").components().any(|c|matches!(c,std::path::Component::Prefix(_))));
}
