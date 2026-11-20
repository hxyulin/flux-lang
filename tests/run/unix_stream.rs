//! Unix stream ownership and I/O match Rustc.
use std::io::{ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;

fn main() {
    let (sender, mut receiver) = UnixStream::pair().unwrap();
    assert_eq!(sender.as_fd().as_raw_fd(), sender.as_raw_fd());
    receiver.set_nonblocking(true).unwrap();
    let mut message = [0; 5];
    assert_eq!(
        receiver.read(&mut message).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    receiver.set_nonblocking(false).unwrap();
    let owned: OwnedFd = sender.into();
    let sender = UnixStream::from(owned);
    let mut copy = sender.try_clone().unwrap();
    copy.write_all(b"hello").unwrap();
    (&receiver).read_exact(&mut message).unwrap();
    sender.shutdown(Shutdown::Write).unwrap();
    assert_eq!(receiver.read(&mut message).unwrap(), 0);
    drop(receiver);
    assert_eq!(
        copy.write(b"closed").unwrap_err().kind(),
        ErrorKind::BrokenPipe
    );
    assert_eq!(
        UnixStream::connect("invalid\0path").unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    println!("{message:?}");
}
