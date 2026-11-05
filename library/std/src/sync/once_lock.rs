//! A bounded, spin-waiting once-initialized cell for sized payloads.

use core::cell::UnsafeCell;
use core::default::Default;
use core::marker::{Send, Sync};
use core::ops::{Drop, FnOnce};
use core::option::Option::{self, None, Some};
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::{Acquire, Release};

/// A value initialized by at most one caller at a time.
///
/// `get` never waits. `get_or_init` spins while another caller initializes;
/// reentrant initialization of the same cell deadlocks. Flux panics abort,
/// so unwinding is not supported by its runtime. The reset guard also makes
/// this implementation retryable when tested with a host unwinding runtime.
/// This bounded API does not implement all of std's OnceLock methods.
pub struct OnceLock<T> {
    // 0 = empty, 1 = initializing, 2 = ready. Only 2 permits payload reads.
    state: AtomicUsize,
    value: UnsafeCell<Option<T>>,
}

// SAFETY: moving the cell transfers ownership of T; shared initialization
// publishes T with release/acquire, and all later payload accesses are shared.
unsafe impl<T: Send> Send for OnceLock<T> {}
// SAFETY: any thread may initialize (requiring Send), and all threads can
// subsequently borrow the published payload (requiring Sync).
unsafe impl<T: Send + Sync> Sync for OnceLock<T> {}

impl<T> OnceLock<T> {
    /// Constructs an empty cell, including in a static initializer.
    pub const fn new() -> OnceLock<T> {
        OnceLock {
            state: AtomicUsize::new(0),
            value: UnsafeCell::new(None),
        }
    }

    /// Returns None when empty or initializing, without waiting.
    pub fn get(&self) -> Option<&T> {
        if self.state.load(Acquire) == 2 {
            // SAFETY: ready was release-published after the only write. The
            // shared borrow prevents destruction while this reference lives.
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }

    /// Publishes value, or returns it when another initializer won. This
    /// waits for an initializer already in progress, like get_or_init.
    pub fn set(&self, value: T) -> Result<(), T> {
        let mut value = Some(value);
        self.get_or_init(|| value.take().unwrap());
        match value { None => Ok(()), Some(value) => Err(value) }
    }

    /// Returns the published value, running f only if this caller wins.
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        loop {
            if let Some(value) = self.get() {
                return value;
            }
            if self.state.compare_exchange(0, 1, Acquire, Acquire).is_ok() {
                let mut guard = Reset {
                    state: &self.state,
                    armed: true,
                };
                let value = f();
                // SAFETY: the successful CAS grants exclusive write access;
                // no reference to the payload exists until ready is published.
                unsafe {
                    *self.value.get() = Some(value);
                }
                self.state.store(2, Release);
                guard.armed = false;
                return self.get().unwrap();
            }
        }
    }

    /// Returns mutable access to the value through an exclusive borrow.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        // An exclusive borrow excludes a running initializer, so the state is
        // empty or ready.
        if *self.state.get_mut() == 2 { self.value.get_mut().as_mut() } else { None }
    }

    /// Removes the value through an exclusive borrow, leaving an empty cell.
    pub fn take(&mut self) -> Option<T> {
        if *self.state.get_mut() == 2 {
            *self.state.get_mut() = 0;
            self.value.get_mut().take()
        } else {
            None
        }
    }

    /// Consumes the cell, returning its value.
    pub fn into_inner(mut self) -> Option<T> {
        self.take()
    }
}

impl<T: core::clone::Clone> core::clone::Clone for OnceLock<T> {
    fn clone(&self) -> OnceLock<T> {
        let cell = OnceLock::new();
        if let Some(value) = self.get() {
            let _ = cell.set(value.clone());
        }
        cell
    }
}

impl<T> Default for OnceLock<T> {
    fn default() -> OnceLock<T> {
        OnceLock::new()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for OnceLock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut tuple = f.debug_tuple("OnceLock");
        match self.get() {
            Some(value) => { tuple.field(value); }
            None => { tuple.field(&format_args!("<uninit>")); }
        }
        tuple.finish()
    }
}

struct Reset<'a> {
    state: &'a AtomicUsize,
    armed: bool,
}

impl<'a> Drop for Reset<'a> {
    fn drop(&mut self) {
        if self.armed {
            self.state.store(0, Release);
        }
    }
}

impl<T: core::panic::UnwindSafe> core::panic::UnwindSafe for OnceLock<T> {}
impl<T: core::panic::UnwindSafe + core::panic::RefUnwindSafe> core::panic::RefUnwindSafe for OnceLock<T> {}
