use std::io::{self, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::net::{UnixDatagram, UnixListener, UnixStream};
use std::path::PathBuf;

struct SocketPath(PathBuf);

impl Drop for SocketPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn socket_path(label: &str) -> SocketPath {
    SocketPath(PathBuf::from(format!(
        "/tmp/flux-socket-{}-{label}",
        std::process::id()
    )))
}

fn pipes() -> io::Result<()> {
    let (reader, writer) = io::pipe()?;
    assert_eq!(reader.as_fd().as_raw_fd(), reader.as_raw_fd());
    let owned: OwnedFd = reader.into();
    let mut reader = io::PipeReader::from(owned);
    let mut clone = writer.try_clone()?;
    drop(writer);
    clone.write_all(b"pipe")?;
    drop(clone);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    assert_eq!(bytes, b"pipe");
    Ok(())
}

fn unix() -> io::Result<()> {
    let stream_path = socket_path("stream");
    let listener = UnixListener::bind(&stream_path.0)?;
    let local = listener.local_addr()?;
    assert_eq!(local.as_pathname(), Some(stream_path.0.as_path()));
    listener.set_nonblocking(true)?;
    assert_eq!(listener.accept().unwrap_err().kind(), ErrorKind::WouldBlock);
    listener.set_nonblocking(false)?;
    let mut client = UnixStream::connect(&stream_path.0)?;
    let (mut server, peer) = listener.accept()?;
    assert!(peer.is_unnamed());
    assert_eq!(client.peer_addr()?.as_pathname(), local.as_pathname());
    client.write_all(b"unix")?;
    let mut buffer = [0; 4];
    server.read_exact(&mut buffer)?;
    assert_eq!(&buffer, b"unix");

    let first = socket_path("first");
    let second = socket_path("second");
    let a = UnixDatagram::bind(&first.0)?;
    let b = UnixDatagram::bind(&second.0)?;
    assert_eq!(a.send_to(b"send", &second.0)?, 4);
    let (len, source) = b.recv_from(&mut buffer)?;
    assert_eq!(source.as_pathname(), Some(first.0.as_path()));
    assert_eq!(&buffer[..len], b"send");
    let (a, b) = UnixDatagram::pair()?;
    a.send(b"pair")?;
    let len = b.recv(&mut buffer)?;
    assert_eq!(&buffer[..len], b"pair");
    Ok(())
}

fn ip() -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let local = listener.local_addr()?;
    let mut client = TcpStream::connect(local)?;
    let (mut server, peer) = listener.accept()?;
    assert_eq!(peer, client.local_addr()?);
    assert_eq!(client.peer_addr()?, local);
    assert!(client.take_error()?.is_none());
    server.write_all(b"tcp")?;
    let mut buffer = [0; 4];
    assert!(client.peek(&mut buffer)? > 0);
    client.read_exact(&mut buffer[..3])?;
    assert_eq!(&buffer[..3], b"tcp");

    let a = UdpSocket::bind("127.0.0.1:0")?;
    let b = UdpSocket::bind("127.0.0.1:0")?;
    assert_eq!(a.send_to(b"udp", b.local_addr()?)?, 3);
    let (len, source) = b.peek_from(&mut buffer)?;
    assert_eq!(&buffer[..len], b"udp");
    assert_eq!(source, a.local_addr()?);
    assert_eq!(b.recv_from(&mut buffer)?, (3, source));
    a.connect(b.local_addr()?)?;
    b.connect(a.local_addr()?)?;
    a.send(b"peer")?;
    assert_eq!(b.peek(&mut buffer)?, 4);
    assert_eq!(b.recv(&mut buffer)?, 4);
    assert_eq!(&buffer, b"peer");
    assert_eq!(b.peer_addr()?, a.local_addr()?);

    let address = "[::1]:0".parse::<SocketAddr>().unwrap();
    if let Ok(socket) = UdpSocket::bind(address) {
        assert_eq!(socket.local_addr()?.ip(), address.ip());
    }
    Ok(())
}

fn main() -> io::Result<()> {
    pipes()?;
    unix()?;
    ip()?;
    println!("pipes, unix sockets, tcp, udp");
    Ok(())
}
