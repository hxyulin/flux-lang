//! A bounded std facade over Flux's implemented core and alloc APIs.
//! Reexports preserve the original definitions and trait implementations.

extern crate alloc as alloc_crate;

#[macro_use]
mod macros;

pub use crate::alloc_crate::{alloc, borrow, boxed, fmt, format, prelude, rc, string, task, vec};
pub use core::{
    any, array, cell, char, clone, cmp, convert, default, error, future, pin, hint, iter, marker,
    mem, num, ops, option, ptr, result,
};
pub use core::{dbg, eprint, eprintln, print, println, stringify, write};
pub use core::{
    assert, assert_eq, assert_ne, cfg, column, compile_error, concat, debug_assert, debug_assert_eq, debug_assert_ne,
    env, file, format_args, include, include_bytes, include_str, line, matches, module_path, option_env, r#try,
    todo, unimplemented, unreachable, writeln,
};
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

/// ASCII operations, and the deprecated AsciiExt trait.
pub mod ascii {
    pub use core::ascii::*;
    use crate::alloc_crate::{string::String, vec::Vec};

    /// ASCII case operations, now inherent on the implementing types.
    #[deprecated(note = "use inherent methods instead")]
    pub trait AsciiExt {
        type Owned;
        fn is_ascii(&self) -> bool;
        fn to_ascii_uppercase(&self) -> Self::Owned;
        fn to_ascii_lowercase(&self) -> Self::Owned;
        fn eq_ignore_ascii_case(&self, other: &Self) -> bool;
        fn make_ascii_uppercase(&mut self);
        fn make_ascii_lowercase(&mut self);
    }
    #[allow(deprecated)]
    impl AsciiExt for u8 {
        type Owned = u8;
        fn is_ascii(&self) -> bool { u8::is_ascii(self) }
        fn to_ascii_uppercase(&self) -> u8 { u8::to_ascii_uppercase(self) }
        fn to_ascii_lowercase(&self) -> u8 { u8::to_ascii_lowercase(self) }
        fn eq_ignore_ascii_case(&self, other: &u8) -> bool { u8::eq_ignore_ascii_case(self, other) }
        fn make_ascii_uppercase(&mut self) { u8::make_ascii_uppercase(self) }
        fn make_ascii_lowercase(&mut self) { u8::make_ascii_lowercase(self) }
    }
    #[allow(deprecated)]
    impl AsciiExt for char {
        type Owned = char;
        fn is_ascii(&self) -> bool { char::is_ascii(self) }
        fn to_ascii_uppercase(&self) -> char { char::to_ascii_uppercase(self) }
        fn to_ascii_lowercase(&self) -> char { char::to_ascii_lowercase(self) }
        fn eq_ignore_ascii_case(&self, other: &char) -> bool { char::eq_ignore_ascii_case(self, other) }
        fn make_ascii_uppercase(&mut self) { char::make_ascii_uppercase(self) }
        fn make_ascii_lowercase(&mut self) { char::make_ascii_lowercase(self) }
    }
    #[allow(deprecated)]
    impl AsciiExt for [u8] {
        type Owned = Vec<u8>;
        fn is_ascii(&self) -> bool { <[u8]>::is_ascii(self) }
        fn to_ascii_uppercase(&self) -> Vec<u8> { let mut bytes = self.to_vec(); bytes.make_ascii_uppercase(); bytes }
        fn to_ascii_lowercase(&self) -> Vec<u8> { let mut bytes = self.to_vec(); bytes.make_ascii_lowercase(); bytes }
        fn eq_ignore_ascii_case(&self, other: &[u8]) -> bool { <[u8]>::eq_ignore_ascii_case(self, other) }
        fn make_ascii_uppercase(&mut self) { <[u8]>::make_ascii_uppercase(self) }
        fn make_ascii_lowercase(&mut self) { <[u8]>::make_ascii_lowercase(self) }
    }
    #[allow(deprecated)]
    impl AsciiExt for str {
        type Owned = String;
        fn is_ascii(&self) -> bool { str::is_ascii(self) }
        fn to_ascii_uppercase(&self) -> String { str::to_ascii_uppercase(self) }
        fn to_ascii_lowercase(&self) -> String { str::to_ascii_lowercase(self) }
        fn eq_ignore_ascii_case(&self, other: &str) -> bool { str::eq_ignore_ascii_case(self, other) }
        fn make_ascii_uppercase(&mut self) { str::make_ascii_uppercase(self) }
        fn make_ascii_lowercase(&mut self) { str::make_ascii_lowercase(self) }
    }
}

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
    #[allow(deprecated)]
    pub use self::once::{Once, OnceState, ONCE_INIT};
    pub use crate::alloc_crate::sync::{Arc, Weak};
    pub use core::sync::atomic;
}
