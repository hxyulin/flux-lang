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
pub use core::{slice, str, u8, u32, f32, f64, primitive};

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
