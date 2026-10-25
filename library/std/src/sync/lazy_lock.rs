//! A lazily initialized value with permanent poisoning on initializer panic.

use core::cell::UnsafeCell;
use core::default::Default;
use core::marker::{Send, Sync};
use core::ops::{Deref, FnOnce};
use core::option::Option::{self, Some};

use super::OnceLock;

pub struct LazyLock<T, F = fn() -> T> {
    value: OnceLock<T>,
    init: UnsafeCell<Option<F>>,
}

// SAFETY: OnceLock serializes initialization and publishes the payload. Only
// the winning thread touches F, and moving F to that thread requires Send.
unsafe impl<T: Send + Sync, F: Send> Sync for LazyLock<T, F> {}

impl<T, F: FnOnce() -> T> LazyLock<T, F> {
    pub const fn new(f: F) -> LazyLock<T, F> {
        LazyLock {
            value: OnceLock::new(),
            init: UnsafeCell::new(Some(f)),
        }
    }

    pub fn force(this: &LazyLock<T, F>) -> &T {
        this.value.get_or_init(|| {
            // SAFETY: only the OnceLock initializer accesses this cell. Taking
            // F before calling it leaves None if it unwinds, poisoning retries.
            let f = unsafe { (*this.init.get()).take() }
                .expect("LazyLock instance has previously been poisoned");
            f()
        })
    }
}

impl<T, F: FnOnce() -> T> Deref for LazyLock<T, F> {
    type Target = T;

    fn deref(&self) -> &T {
        LazyLock::force(self)
    }
}

impl<T: Default> Default for LazyLock<T> {
    fn default() -> LazyLock<T> {
        LazyLock::new(T::default)
    }
}
