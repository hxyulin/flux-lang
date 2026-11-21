//! Native operations shared across socket address families.
use crate::fs::File;
use crate::io::{Error, Result};
use crate::os::fd::{AsRawFd, FromRawFd};

#[cfg(target_os = "linux")]
const CLOEXEC: i32 = 0o2000000;
#[cfg(target_os = "macos")]
const CLOEXEC: i32 = 0;
#[cfg(target_os = "linux")]
pub(super) const NO_SIGNAL: i32 = 0x4000;
#[cfg(target_os = "macos")]
pub(super) const NO_SIGNAL: i32 = 0x80000;
pub(super) const PEEK: i32 = 2;

unsafe extern "C" {
    fn socket(domain: i32, kind: i32, protocol: i32) -> i32;
    fn socketpair(domain: i32, kind: i32, protocol: i32, fds: *mut i32) -> i32;
    pub(super) fn bind(fd: i32, addr: *const u8, len: u32) -> i32;
    pub(super) fn connect(fd: i32, addr: *const u8, len: u32) -> i32;
    pub(super) fn listen(fd: i32, backlog: i32) -> i32;
    pub(super) fn getsockname(fd: i32, addr: *mut u8, len: *mut u32) -> i32;
    pub(super) fn getpeername(fd: i32, addr: *mut u8, len: *mut u32) -> i32;
    pub(super) fn getsockopt(
        fd: i32,
        level: i32,
        option: i32,
        value: *mut u8,
        len: *mut u32,
    ) -> i32;
    #[cfg(target_os = "macos")]
    fn accept(fd: i32, addr: *mut u8, len: *mut u32) -> i32;
    #[cfg(target_os = "linux")]
    fn accept4(fd: i32, addr: *mut u8, len: *mut u32, flags: i32) -> i32;
    pub(super) fn send(fd: i32, buf: *const u8, len: usize, flags: i32) -> isize;
    pub(super) fn recv(fd: i32, buf: *mut u8, len: usize, flags: i32) -> isize;
    pub(super) fn sendto(
        fd: i32,
        buf: *const u8,
        len: usize,
        flags: i32,
        addr: *const u8,
        addrlen: u32,
    ) -> isize;
    pub(super) fn recvfrom(
        fd: i32,
        buf: *mut u8,
        len: usize,
        flags: i32,
        addr: *mut u8,
        addrlen: *mut u32,
    ) -> isize;
    pub(super) fn shutdown(fd: i32, how: i32) -> i32;
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

pub(super) fn status(code: i32) -> Result<()> {
    if code < 0 {
        Err(Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(super) fn byte_count(count: isize) -> Result<usize> {
    if count < 0 {
        Err(Error::last_os_error())
    } else {
        Ok(count as usize)
    }
}

fn finish(file: File) -> Result<File> {
    #[cfg(target_os = "macos")]
    // SAFETY: this is a new live descriptor. F_SETFD takes an integer flag.
    status(unsafe { fcntl(file.as_raw_fd(), 2, 1i32) })?;
    Ok(file)
}

pub(super) fn socket_file(domain: i32, kind: i32) -> Result<File> {
    // SAFETY: a successful socket call transfers a new descriptor.
    let fd = unsafe { socket(domain, kind | CLOEXEC, 0) };
    status(fd)?;
    // SAFETY: this is the unique owner of the successful descriptor.
    finish(unsafe { File::from_raw_fd(fd) })
}

pub(super) fn socket_pair(domain: i32, kind: i32) -> Result<(File, File)> {
    let mut fds = [-1; 2];
    // SAFETY: fds has two writable slots for the newly owned descriptors.
    status(unsafe { socketpair(domain, kind | CLOEXEC, 0, fds.as_mut_ptr()) })?;
    // SAFETY: successful socketpair returned two distinct owned descriptors.
    let (a, b) = unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) };
    Ok((finish(a)?, finish(b)?))
}

/// Accepts a socket, writing its address within the caller's output storage.
///
/// # Safety
/// The caller must provide a live listener and a writable buffer of *len bytes.
pub(super) unsafe fn accept_file(fd: i32, addr: *mut u8, len: *mut u32) -> Result<File> {
    loop {
        // SAFETY: the caller keeps the address and length live and writable.
        #[cfg(target_os = "macos")]
        let accepted = unsafe { accept(fd, addr, len) };
        #[cfg(target_os = "linux")]
        let accepted = unsafe { accept4(fd, addr, len, CLOEXEC) };
        if accepted >= 0 {
            // SAFETY: successful accept transferred a new owned descriptor.
            return finish(unsafe { File::from_raw_fd(accepted) });
        }
        let error = Error::last_os_error();
        if error.kind() != crate::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

pub(super) fn set_nonblocking(fd: i32, nonblocking: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    const NONBLOCK: i32 = 0x4;
    #[cfg(target_os = "linux")]
    const NONBLOCK: i32 = 0o4000;
    // SAFETY: F_GETFL reads flags and takes no trailing argument.
    let flags = unsafe { fcntl(fd, 3) };
    status(flags)?;
    let flags = if nonblocking {
        flags | NONBLOCK
    } else {
        flags & !NONBLOCK
    };
    // SAFETY: F_SETFL takes an integer containing the updated flags.
    status(unsafe { fcntl(fd, 4, flags) })
}
