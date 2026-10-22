//! Printing, which rustc's programs get from `std`: `print!` and
//! `println!` write to stdout, buffered by lines as std's `Stdout` is
//! (a `LineWriter` of 1024 bytes), and `eprint!` and `eprintln!` to
//! stderr, unbuffered. What stdout holds when `main` returns is
//! written then; a panic, which aborts, loses it, as with rustc's
//! `panic=abort`.

use crate::fmt::{self, Arguments, Write};
use crate::panic::Location;
use crate::result::Result::Ok;

unsafe extern "C" {
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    fn pthread_threadid_np(thread: usize, id: *mut u64) -> i32;
}

/// How many bytes std's stdout buffers.
const CAPACITY: usize = 1024;

static mut STDOUT_BUF: [u8; CAPACITY] = [0; CAPACITY];
static mut STDOUT_LEN: usize = 0;

/// Writes all of `len` bytes at `data` to `fd`.
fn write_all(fd: i32, data: *const u8, len: usize) {
    let mut done = 0;
    while done < len {
        let n = unsafe { write(fd, (data as usize + done) as *const u8, len - done) };
        if n <= 0 {
            return;
        }
        done += n as usize;
    }
}

/// Writes out what stdout holds.
#[lang = "flush_stdout"]
#[doc(hidden)]
pub fn flush_stdout() {
    unsafe {
        if STDOUT_LEN > 0 {
            // A copy, as nothing may borrow a `static mut`.
            let held = STDOUT_BUF;
            write_all(1, held.as_ptr(), STDOUT_LEN);
            STDOUT_LEN = 0;
        }
    }
}

/// `BufWriter::write`: buffers `bytes`, writing out what is held first
/// if they do not fit, or writing them directly if they never would.
fn buffer(bytes: &[u8]) {
    unsafe {
        if STDOUT_LEN + bytes.len() > CAPACITY {
            flush_stdout();
        }
        if bytes.len() >= CAPACITY {
            write_all(1, bytes.as_ptr(), bytes.len());
        } else {
            let mut i = 0;
            while i < bytes.len() {
                STDOUT_BUF[STDOUT_LEN + i] = bytes[i];
                i += 1;
            }
            STDOUT_LEN += bytes.len();
        }
    }
}

/// `LineWriter::write_all`: what ends a line is written out, with what
/// was held before it; the rest is held.
fn stdout_write(bytes: &[u8]) {
    let mut last = bytes.len();
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if bytes[i] == b'\n' {
            last = i;
            break;
        }
    }
    if last == bytes.len() {
        unsafe {
            if STDOUT_LEN > 0 && STDOUT_BUF[STDOUT_LEN - 1] == b'\n' {
                flush_stdout();
            }
        }
        buffer(bytes);
        return;
    }
    flush_stdout();
    write_all(1, bytes.as_ptr(), last + 1);
    let tail = unsafe { crate::intrinsics::subslice(bytes, last + 1, bytes.len()) };
    if !tail.is_empty() {
        buffer(tail);
    }
}

struct Stdout;

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        stdout_write(s.as_bytes());
        Ok(())
    }
}

struct Stderr;

impl Write for Stderr {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_all(2, s.as_ptr(), s.len());
        Ok(())
    }
}

/// `print!`
#[doc(hidden)]
pub fn _print(args: Arguments<'_>) {
    let _ = fmt::write(&mut Stdout, args);
}

/// `eprint!`
#[doc(hidden)]
pub fn _eprint(args: Arguments<'_>) {
    let _ = fmt::write(&mut Stderr, args);
}

/// What std's default panic hook prints, for a panic at `location`
/// with message `msg`: the thread is always `main`.
pub(crate) fn print_panic(msg: Arguments<'_>, location: &Location<'_>) {
    let mut tid = 0u64;
    unsafe {
        pthread_threadid_np(0, &mut tid);
    }
    let _ = fmt::write(
        &mut Stderr,
        format_args!("\nthread 'main' ({tid}) panicked at {location}:\n{msg}\n"),
    );
    let _ = Stderr.write_str(
        "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n",
    );
}
