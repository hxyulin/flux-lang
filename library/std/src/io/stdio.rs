//! Borrowed standard descriptors, shared input buffering and output locks.
use super::{BufRead, BufReader, Error, ErrorKind, Read, Result, Write};
use crate::alloc_crate::string::String;
use crate::alloc_crate::vec::Vec;
use crate::os::fd::AsRawFd;
use crate::sync::{LazyLock, Mutex, MutexGuard};
use core::marker::PhantomData;

unsafe extern "C" {
    fn read(fd: i32, data: *mut u8, length: usize) -> isize;
    fn isatty(fd: i32) -> i32;
}
fn read_stdin(output: &mut [u8]) -> Result<usize> {
    // SAFETY: descriptor zero is borrowed, not closed or transferred; output
    // supplies initialized writable storage of the bounded native length.
    let count = unsafe {
        read(
            0,
            output.as_mut_ptr(),
            output.len().min(isize::MAX as usize),
        )
    };
    if count < 0 {
        let error = Error::last_os_error();
        if error.raw_os_error() == Some(9) {
            Ok(0)
        } else {
            Err(error)
        }
    } else {
        Ok(count as usize)
    }
}
fn output_error(code: i32) -> Error {
    if code == 0 {
        ErrorKind::WriteZero.into()
    } else {
        Error::from_raw_os_error(code)
    }
}
struct StdinRaw;
impl Read for StdinRaw {
    fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        read_stdin(output)
    }
}
static INPUT: LazyLock<Mutex<BufReader<StdinRaw>>> =
    LazyLock::new(|| Mutex::new(BufReader::new(StdinRaw)));

/// A handle to the process's shared buffered standard input.
#[derive(Clone)]
pub struct Stdin {
    _private: (),
}
pub fn stdin() -> Stdin {
    Stdin { _private: () }
}
impl Stdin {
    pub fn lock(&self) -> StdinLock<'static> {
        StdinLock {
            guard: INPUT.lock().unwrap_or_else(|error| error.into_inner()),
        }
    }
    pub fn read_line(&self, output: &mut String) -> Result<usize> {
        self.lock().read_line(output)
    }
}
impl Read for Stdin {
    fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        self.lock().read(output)
    }
    fn read_exact(&mut self, output: &mut [u8]) -> Result<()> {
        self.lock().read_exact(output)
    }
    fn read_to_end(&mut self, output: &mut Vec<u8>) -> Result<usize> {
        self.lock().read_to_end(output)
    }
    fn read_to_string(&mut self, output: &mut String) -> Result<usize> {
        self.lock().read_to_string(output)
    }
}
impl Read for &Stdin {
    fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        self.lock().read(output)
    }
    fn read_exact(&mut self, output: &mut [u8]) -> Result<()> {
        self.lock().read_exact(output)
    }
    fn read_to_end(&mut self, output: &mut Vec<u8>) -> Result<usize> {
        self.lock().read_to_end(output)
    }
    fn read_to_string(&mut self, output: &mut String) -> Result<usize> {
        self.lock().read_to_string(output)
    }
}
/// An exclusive borrow of the shared stdin buffer; it stays on its thread.
pub struct StdinLock<'a> {
    guard: MutexGuard<'a, BufReader<StdinRaw>>,
}
impl Read for StdinLock<'_> {
    fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        self.guard.read(output)
    }
    fn read_exact(&mut self, output: &mut [u8]) -> Result<()> {
        self.guard.read_exact(output)
    }
    fn read_to_end(&mut self, output: &mut Vec<u8>) -> Result<usize> {
        self.guard.read_to_end(output)
    }
    fn read_to_string(&mut self, output: &mut String) -> Result<usize> {
        self.guard.read_to_string(output)
    }
}
impl BufRead for StdinLock<'_> {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        self.guard.fill_buf()
    }
    fn consume(&mut self, amount: usize) {
        self.guard.consume(amount);
    }
    fn read_until(&mut self, delimiter: u8, output: &mut Vec<u8>) -> Result<usize> {
        self.guard.read_until(delimiter, output)
    }
    fn read_line(&mut self, output: &mut String) -> Result<usize> {
        self.guard.read_line(output)
    }
}

/// A handle to stdout's process-wide reentrant line-buffered stream.
#[derive(Clone)]
pub struct Stdout {
    _private: (),
}
pub fn stdout() -> Stdout {
    Stdout { _private: () }
}
impl Stdout {
    pub fn lock(&self) -> StdoutLock<'static> {
        StdoutLock {
            _guard: core::io::lock_stdout(),
            marker: PhantomData,
        }
    }
}
/// A thread-bound reentrant guard sharing the print macros' stdout buffer.
pub struct StdoutLock<'a> {
    _guard: core::io::OutputGuard,
    marker: PhantomData<&'a ()>,
}
impl Write for Stdout {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        self.lock().write(bytes)
    }
    fn flush(&mut self) -> Result<()> {
        self.lock().flush()
    }
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.lock().write_all(bytes)
    }
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<()> {
        self.lock().write_fmt(args)
    }
}
impl Write for &Stdout {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        self.lock().write(bytes)
    }
    fn flush(&mut self) -> Result<()> {
        self.lock().flush()
    }
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.lock().write_all(bytes)
    }
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<()> {
        self.lock().write_fmt(args)
    }
}
impl Write for StdoutLock<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        core::io::write_stdout(bytes).map_err(output_error)
    }
    fn flush(&mut self) -> Result<()> {
        core::io::flush_stdout_checked().map_err(output_error)
    }
}

/// A handle to the process-wide reentrant, unbuffered standard error stream.
#[derive(Clone)]
pub struct Stderr {
    _private: (),
}
pub fn stderr() -> Stderr {
    Stderr { _private: () }
}
impl Stderr {
    pub fn lock(&self) -> StderrLock<'static> {
        StderrLock {
            _guard: core::io::lock_stderr(),
            marker: PhantomData,
        }
    }
}
/// A thread-bound reentrant guard sharing the print macros' stderr stream.
pub struct StderrLock<'a> {
    _guard: core::io::OutputGuard,
    marker: PhantomData<&'a ()>,
}
impl Write for Stderr {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        self.lock().write(bytes)
    }
    fn flush(&mut self) -> Result<()> {
        self.lock().flush()
    }
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.lock().write_all(bytes)
    }
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<()> {
        self.lock().write_fmt(args)
    }
}
impl Write for &Stderr {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        self.lock().write(bytes)
    }
    fn flush(&mut self) -> Result<()> {
        self.lock().flush()
    }
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.lock().write_all(bytes)
    }
    fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<()> {
        self.lock().write_fmt(args)
    }
}
impl Write for StderrLock<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        core::io::write_stderr(bytes).map_err(output_error)
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl core::fmt::Debug for Stdin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Stdin").finish_non_exhaustive()
    }
}
impl core::fmt::Debug for Stdout {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Stdout").finish_non_exhaustive()
    }
}
impl core::fmt::Debug for Stderr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Stderr").finish_non_exhaustive()
    }
}
impl core::fmt::Debug for StdinLock<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StdinLock").finish_non_exhaustive()
    }
}
impl core::fmt::Debug for StdoutLock<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StdoutLock").finish_non_exhaustive()
    }
}
impl core::fmt::Debug for StderrLock<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StderrLock").finish_non_exhaustive()
    }
}
impl AsRawFd for Stdin {
    fn as_raw_fd(&self) -> i32 {
        0
    }
}
impl AsRawFd for StdinLock<'_> {
    fn as_raw_fd(&self) -> i32 {
        0
    }
}
impl AsRawFd for Stdout {
    fn as_raw_fd(&self) -> i32 {
        1
    }
}
impl AsRawFd for StdoutLock<'_> {
    fn as_raw_fd(&self) -> i32 {
        1
    }
}
impl AsRawFd for Stderr {
    fn as_raw_fd(&self) -> i32 {
        2
    }
}
impl AsRawFd for StderrLock<'_> {
    fn as_raw_fd(&self) -> i32 {
        2
    }
}

mod terminal_sealed {
    pub trait Sealed {}
}
/// Whether a supported file or standard stream refers to a live terminal.
pub trait IsTerminal: terminal_sealed::Sealed {
    fn is_terminal(&self) -> bool;
}
fn terminal(fd: i32) -> bool {
    // SAFETY: isatty only queries the borrowed descriptor and reports false for
    // closed descriptors, files and pipes. It does not transfer ownership.
    unsafe { isatty(fd) == 1 }
}
impl terminal_sealed::Sealed for crate::fs::File {}
impl IsTerminal for crate::fs::File {
    fn is_terminal(&self) -> bool {
        terminal(self.raw_fd())
    }
}
impl terminal_sealed::Sealed for Stdin {}
impl IsTerminal for Stdin {
    fn is_terminal(&self) -> bool {
        terminal(0)
    }
}
impl terminal_sealed::Sealed for StdinLock<'_> {}
impl IsTerminal for StdinLock<'_> {
    fn is_terminal(&self) -> bool {
        terminal(0)
    }
}
impl terminal_sealed::Sealed for Stdout {}
impl IsTerminal for Stdout {
    fn is_terminal(&self) -> bool {
        terminal(1)
    }
}
impl terminal_sealed::Sealed for StdoutLock<'_> {}
impl IsTerminal for StdoutLock<'_> {
    fn is_terminal(&self) -> bool {
        terminal(1)
    }
}
impl terminal_sealed::Sealed for Stderr {}
impl IsTerminal for Stderr {
    fn is_terminal(&self) -> bool {
        terminal(2)
    }
}
impl terminal_sealed::Sealed for StderrLock<'_> {}
impl IsTerminal for StderrLock<'_> {
    fn is_terminal(&self) -> bool {
        terminal(2)
    }
}
