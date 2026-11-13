use std::io::Write;
use std::os::fd::{AsFd, AsRawFd};
use std::process::{Command, Stdio};
fn same_fd<T: AsFd + AsRawFd>(value: &T) { assert_eq!(value.as_fd().as_raw_fd(), value.as_raw_fd()); }
fn main() {
    let mut child=Command::new("/bin/sh").args(["-c","cat; printf error >&2"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut input=child.stdin.take().unwrap();
    same_fd(&input); same_fd(child.stdout.as_ref().unwrap()); same_fd(child.stderr.as_ref().unwrap());
    input.write_all(b"hello").unwrap(); drop(input);
    let output=child.wait_with_output().unwrap(); assert!(output.status.success());
    assert_eq!(output.stdout,b"hello"); assert_eq!(output.stderr,b"error"); println!("pipes borrowed");
}
