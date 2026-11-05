//! A bounded std facade over Flux's implemented core and alloc APIs.
//! Reexports preserve the original definitions and trait implementations.

extern crate alloc as alloc_crate;

#[macro_use]
mod macros;

pub use crate::alloc_crate::{alloc, borrow, boxed, fmt, format, prelude, rc, string, vec};
pub use core::{
    any, array, ascii, cell, char, clone, cmp, convert, default, error, future, pin, task, hint, iter, marker,
    mem, num, ops, option, ptr, result,
};
pub use core::{dbg, eprint, eprintln, print, println, stringify, write};
pub use core::{slice, str, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, primitive};

mod float;

/// Panic support. Flux compiles with panic=abort, so no panic reaches a
/// caller of catch_unwind.
pub mod panic {
    pub use core::panic::*;
    /// Calls `f`. A panic inside it aborts the process, as under rustc's
    /// panic=abort, so the result is always Ok.
    pub fn catch_unwind<F: FnOnce() -> R + UnwindSafe, R>(f: F) -> crate::thread::Result<R> {
        Ok(f())
    }
    /// Aborts the process: with panic=abort there is no unwinding to resume.
    pub fn resume_unwind(payload: crate::boxed::Box<dyn core::any::Any + Send>) -> ! {
        drop(payload);
        core::panicking::abort_now()
    }
}

pub mod arch;

/// Stack backtraces. Flux cannot capture frames: a capture that the
/// environment enables, or that is forced, reports Unsupported.
pub mod backtrace {
    /// Whether a backtrace holds frames.
    #[non_exhaustive]
    #[derive(Debug, PartialEq, Eq)]
    pub enum BacktraceStatus {
        Unsupported,
        Disabled,
        Captured,
    }
    /// A captured stack backtrace; in Flux it never holds frames.
    pub struct Backtrace { status: BacktraceStatus }
    impl Backtrace {
        // RUST_LIB_BACKTRACE, then RUST_BACKTRACE, enable captures unless "0".
        fn enabled() -> bool {
            match crate::env::var("RUST_LIB_BACKTRACE") {
                Ok(s) => s != "0",
                Err(_) => match crate::env::var("RUST_BACKTRACE") {
                    Ok(s) => s != "0",
                    Err(_) => false,
                },
            }
        }
        pub fn capture() -> Backtrace {
            Backtrace { status: if Backtrace::enabled() { BacktraceStatus::Unsupported } else { BacktraceStatus::Disabled } }
        }
        pub fn force_capture() -> Backtrace { Backtrace { status: BacktraceStatus::Unsupported } }
        pub const fn disabled() -> Backtrace { Backtrace { status: BacktraceStatus::Disabled } }
        pub fn status(&self) -> BacktraceStatus {
            match self.status {
                BacktraceStatus::Disabled => BacktraceStatus::Disabled,
                _ => BacktraceStatus::Unsupported,
            }
        }
    }
    impl core::fmt::Display for Backtrace {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self.status {
                BacktraceStatus::Disabled => f.write_str("disabled backtrace"),
                _ => f.write_str("unsupported backtrace"),
            }
        }
    }
    impl core::fmt::Debug for Backtrace {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self.status {
                BacktraceStatus::Disabled => f.write_str("<disabled>"),
                _ => f.write_str("<unsupported>"),
            }
        }
    }
}

pub mod thread;
pub mod time;
pub mod net;
pub mod os;
pub mod process;
pub mod collections;
pub mod env;
pub mod ffi;
pub mod path;
pub mod fs;
pub mod io;
pub mod hash;

pub mod sync {
    #[cfg(any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64")))]
    pub mod mpsc;
    mod once_lock;
    mod once;
    mod lazy_lock;
    mod mutex;
    mod rwlock;
    mod poison;
    pub use self::lazy_lock::LazyLock;
    pub use self::mutex::{Condvar, Mutex, MutexGuard, WaitTimeoutResult};
    pub(crate) use self::mutex::native;
    pub use self::rwlock::{RwLock, RwLockReadGuard, RwLockWriteGuard};
    pub use self::poison::{LockResult, PoisonError, TryLockError, TryLockResult};
    pub use self::once_lock::OnceLock;
    pub use self::once::{Once, OnceState};
    pub use crate::alloc_crate::sync::{Arc, Weak};
    pub use core::sync::atomic;
}
