//! A bounded, spin-waiting once-initialized cell for sized payloads.

use core::cell::UnsafeCell;
use core::default::Default;
use core::marker::{Send, Sync};
use core::mem::MaybeUninit;
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
    value: UnsafeCell<MaybeUninit<T>>,
}

// SAFETY: moving the cell transfers ownership of T; shared initialization
// publishes T with release/acquire, and all later payload accesses are shared.
unsafe impl<T: Send> Send for OnceLock<T> {}
// SAFETY: any thread may initialize (requiring Send), and all threads can
// subsequently borrow the published payload (requiring Sync).
unsafe impl<T: Send + Sync> Sync for OnceLock<T> {}

impl<T> OnceLock<T> {
    /// Constructs an empty cell. Flux cannot yet evaluate the contained
    /// MaybeUninit construction in a static initializer.
    pub const fn new() -> OnceLock<T> {
        OnceLock {
            state: AtomicUsize::new(0),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Returns None when empty or initializing, without waiting.
    pub fn get(&self) -> Option<&T> {
        if self.state.load(Acquire) == 2 {
            // SAFETY: ready was release-published after the only write. The
            // shared borrow prevents destruction while this reference lives.
            Some(unsafe { (*self.value.get()).assume_init_ref() })
        } else {
            None
        }
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
                    (*self.value.get()).write(value);
                }
                self.state.store(2, Release);
                guard.armed = false;
                // SAFETY: this caller initialized and published the payload.
                return unsafe { (*self.value.get()).assume_init_ref() };
            }
        }
    }
}

impl<T> Default for OnceLock<T> {
    fn default() -> OnceLock<T> {
        OnceLock::new()
    }
}

impl<T> Drop for OnceLock<T> {
    fn drop(&mut self) {
        if *self.state.get_mut() == 2 {
            // SAFETY: exclusive access rules out concurrent users. Only ready
            // holds a T, and MaybeUninit's own drop never drops it again.
            unsafe {
                self.value.get_mut().assume_init_drop();
            }
        }
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
