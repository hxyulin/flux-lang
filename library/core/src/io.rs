//! Shared line-buffered stdout and unbuffered stderr for printing and stdio.
//! Output guards are reentrant on their owning pthread and serialize access
//! to the stdout buffer across threads. Checked byte hooks retain unwritten
//! buffered data on errors; the print macros keep their formatting interface.
use crate::cell::UnsafeCell;
use crate::fmt::{self, Arguments, Write};
use crate::marker::PhantomData;
use crate::panic::Location;
use crate::result::Result;
use crate::result::Result::{Err, Ok};
use crate::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

unsafe extern "C" {
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    fn pthread_self() -> usize;

}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn pthread_threadid_np(thread: usize, id: *mut u64) -> i32;
    fn __error() -> *mut i32;
}
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn __errno_location() -> *mut i32;
    fn gettid() -> i32;
}

struct OutputLock {
    owner: AtomicUsize,
    depth: UnsafeCell<usize>,
}
// SAFETY: depth is accessed only by the pthread whose nonzero identity owns
// the acquire/release lock. Reentrant accesses occur sequentially on that thread.
unsafe impl crate::marker::Sync for OutputLock {}
impl OutputLock {
    const fn new() -> OutputLock {
        OutputLock {
            owner: AtomicUsize::new(0),
            depth: UnsafeCell::new(0),
        }
    }
    fn lock(&'static self) -> OutputGuard {
        // SAFETY: on supported Unix targets pthread_t has pointer/usize ABI;
        // pthread_self returns the current live thread's nonzero identity.
        let current = unsafe { pthread_self() };
        assert!(current != 0, "pthread identity must be nonzero");
        loop {
            if self.owner.load(Ordering::Acquire) == current {
                break;
            }
            if self
                .owner
                .compare_exchange(0, current, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
        // SAFETY: this thread owns the lock, including recursive acquisitions.
        unsafe {
            *self.depth.get() = (*self.depth.get())
                .checked_add(1)
                .expect("output lock recursion overflow");
        }
        OutputGuard {
            lock: self,
            marker: PhantomData,
        }
    }
}
/// A thread-bound shared-output lock. It exposes no buffer references.
#[doc(hidden)]
pub struct OutputGuard {
    lock: &'static OutputLock,
    marker: PhantomData<*mut ()>,
}
impl crate::ops::Drop for OutputGuard {
    fn drop(&mut self) {
        // SAFETY: !Send keeps this guard on the pthread holding the lock.
        unsafe {
            *self.lock.depth.get() -= 1;
            if *self.lock.depth.get() == 0 {
                self.lock.owner.store(0, Ordering::Release);
            }
        }
    }
}
static STDOUT_LOCK: OutputLock = OutputLock::new();
static STDERR_LOCK: OutputLock = OutputLock::new();
#[doc(hidden)]
pub fn lock_stdout() -> OutputGuard {
    STDOUT_LOCK.lock()
}
#[doc(hidden)]
pub fn lock_stderr() -> OutputGuard {
    STDERR_LOCK.lock()
}

const CAPACITY: usize = 1024;
static mut STDOUT_BUF: [u8; CAPACITY] = [0; CAPACITY];
static mut STDOUT_LEN: usize = 0;

fn errno() -> i32 {
    // SAFETY: libc returns this pthread's live errno slot.
    unsafe {
        #[cfg(target_os = "macos")]
        {
            *__error()
        }
        #[cfg(target_os = "linux")]
        {
            *__errno_location()
        }
    }
}
fn write_once(fd: i32, bytes: &[u8]) -> Result<usize, i32> {
    loop {
        // SAFETY: bytes supplies readable storage for the bounded native write.
        let count = unsafe {
            write(
                fd,
                bytes.as_ptr(),
                crate::cmp::min(bytes.len(), isize::MAX as usize),
            )
        };
        if count >= 0 {
            return Ok(count as usize);
        }
        let error = errno();
        // Closed standard streams are intentionally treated as sinks, matching
        // stdio's process-startup/daemon behavior. Other errors remain visible.
        if error == 9 && (fd == 1 || fd == 2) {
            return Ok(bytes.len());
        }
        if error != 4 {
            return Err(error);
        }
    }
}
fn write_all(fd: i32, bytes: &[u8]) -> Result<(), i32> {
    let mut done = 0;
    while done < bytes.len() {
        let count = write_once(fd, &bytes[done..])?;
        if count == 0 {
            return Err(0);
        }
        done += count;
    }
    Ok(())
}
fn flush_locked() -> Result<(), i32> {
    // SAFETY: every caller retains STDOUT_LOCK. Copying the bytes avoids
    // references to static mut; each successful prefix is removed immediately,
    // so a later error leaves only the still-unwritten suffix buffered.
    unsafe {
        while STDOUT_LEN > 0 {
            let held = STDOUT_BUF;
            let count = write_once(1, &held[..STDOUT_LEN])?;
            if count == 0 {
                return Err(0);
            }
            let remaining = STDOUT_LEN - count;
            let mut index = 0;
            while index < remaining {
                STDOUT_BUF[index] = held[count + index];
                index += 1;
            }
            STDOUT_LEN = remaining;
        }
    }
    Ok(())
}
/// Flush shared stdout without discarding errors or unwritten bytes.
#[doc(hidden)]
pub fn flush_stdout_checked() -> Result<(), i32> {
    let _guard = lock_stdout();
    flush_locked()
}
/// Writes out what stdout holds when the runtime exits normally.
#[lang = "flush_stdout"]
#[doc(hidden)]
pub fn flush_stdout() {
    let _ = flush_stdout_checked();
}

/// Write through the shared stdout line buffer. Zero error denotes WriteZero;
/// other errors are native errno values. A successful count describes exactly
/// the prefix accepted from this input.
#[doc(hidden)]
pub fn write_stdout(bytes: &[u8]) -> Result<usize, i32> {
    let _guard = lock_stdout();
    if bytes.is_empty() {
        return Ok(0);
    }
    if let Some(last) = bytes.iter().rposition(|&byte| byte == b'\n') {
        flush_locked()?;
        let prefix = last + 1;
        let count = write_once(1, &bytes[..prefix])?;
        if count != prefix {
            return Ok(count);
        }
        let tail = &bytes[prefix..];
        if tail.len() >= CAPACITY {
            // Report only the prefix: the caller retries the unbuffered tail.
            return Ok(prefix);
        }
        // SAFETY: STDOUT_LOCK is held, the prior buffer is empty, and tail fits.
        unsafe {
            let mut index = 0;
            while index < tail.len() {
                STDOUT_BUF[index] = tail[index];
                index += 1;
            }
            STDOUT_LEN = tail.len();
        }
        return Ok(bytes.len());
    }
    // SAFETY: shared output state is protected by the live guard.
    if unsafe { STDOUT_LEN } + bytes.len() > CAPACITY {
        flush_locked()?;
    }
    if bytes.len() >= CAPACITY {
        return write_once(1, bytes);
    }
    // SAFETY: the guarded buffer has enough room after the preceding flush.
    unsafe {
        let mut index = 0;
        while index < bytes.len() {
            STDOUT_BUF[STDOUT_LEN + index] = bytes[index];
            index += 1;
        }
        STDOUT_LEN += bytes.len();
    }
    Ok(bytes.len())
}
#[doc(hidden)]
pub fn write_stderr(bytes: &[u8]) -> Result<usize, i32> {
    let _guard = lock_stderr();
    write_once(2, bytes)
}

struct Stdout;
impl Write for Stdout {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let mut done = 0;
        while done < text.len() {
            match write_stdout(&text.as_bytes()[done..]) {
                Ok(0) | Err(_) => return Err(fmt::Error),
                Ok(count) => done += count,
            }
        }
        Ok(())
    }
}
struct Stderr;
impl Write for Stderr {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        write_all(2, text.as_bytes()).map_err(|_| fmt::Error)
    }
}

/// `print!`
#[doc(hidden)]
pub fn _print(args: Arguments<'_>) {
    let _guard = lock_stdout();
    let _ = fmt::write(&mut Stdout, args);
}

/// `eprint!`
#[doc(hidden)]
pub fn _eprint(args: Arguments<'_>) {
    let _guard = lock_stderr();
    let _ = fmt::write(&mut Stderr, args);
}

static PANIC_NOTE: AtomicBool = AtomicBool::new(false);

/// The default hook's message, serialized with the other stderr writers.
pub(crate) fn print_panic(msg: Arguments<'_>, location: &Location<'_>, name: &str) {
    let _guard = lock_stderr();
    #[cfg(target_os = "macos")]
    let tid = {
        let mut tid = 0u64;
        // SAFETY: the live local receives this pthread's Darwin thread identifier.
        unsafe { pthread_threadid_np(0, &mut tid); }
        tid
    };
    #[cfg(target_os = "linux")]
    // SAFETY: gettid takes no arguments and returns the calling Linux thread ID.
    let tid = unsafe { gettid() as u64 };
    let _ = fmt::write(
        &mut Stderr,
        format_args!("\nthread '{name}' ({tid}) panicked at {location}:\n{msg}\n"),
    );
    if !PANIC_NOTE.swap(true, Ordering::Relaxed) {
        let _ = Stderr.write_str(
            "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n",
        );
    }
}
