//! A spin-waiting mutex. Recursive locking deadlocks; scheduling is not fair.

use core::cell::UnsafeCell;
use core::default::Default;
use core::marker::{PhantomData, Send, Sync};
use core::ops::{Deref, DerefMut, Drop};
use core::result::Result::{Err, Ok};
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use super::{LockResult, PoisonError};

pub struct Mutex<T: ?Sized> {
    locked: AtomicUsize,
    poisoned: AtomicUsize,
    value: UnsafeCell<T>,
}

// SAFETY: moving the mutex transfers ownership of T.
unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
// SAFETY: the atomic lock allows only one thread to access T at a time.
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Mutex<T> {
        Mutex {
            locked: AtomicUsize::new(0),
            poisoned: AtomicUsize::new(0),
            value: UnsafeCell::new(value),
        }
    }

    pub fn into_inner(self) -> LockResult<T> {
        if self.is_poisoned() {
            Err(PoisonError::new(self.value.into_inner()))
        } else {
            Ok(self.value.into_inner())
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> LockResult<MutexGuard<'_, T>> {
        while self.locked.compare_exchange(0, 1, Acquire, Relaxed).is_err() {}
        let guard = MutexGuard {
            lock: self,
            panicking: crate::thread::panicking(),
            marker: PhantomData,
        };
        if self.is_poisoned() { Err(PoisonError::new(guard)) } else { Ok(guard) }
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned.load(Relaxed) != 0
    }

    pub fn clear_poison(&self) {
        self.poisoned.store(0, Relaxed);
    }

    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        if self.is_poisoned() {
            Err(PoisonError::new(self.value.get_mut()))
        } else {
            Ok(self.value.get_mut())
        }
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Mutex<T> { Mutex::new(T::default()) }
}

impl<T: core::fmt::Debug> core::fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut debug = f.debug_struct("Mutex");
        if self.locked.compare_exchange(0, 1, Acquire, Relaxed).is_ok() {
            let guard = MutexGuard {
                lock: self,
                panicking: crate::thread::panicking(),
                marker: PhantomData,
            };
            debug.field("data", &*guard);
        } else {
            debug.field("data", &"<locked>");
        }
        debug.field("poisoned", &self.is_poisoned()).finish_non_exhaustive()
    }
}

pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
    panicking: bool,
    // Native lock guards must not move to another thread.
    marker: PhantomData<*mut ()>,
}

// SAFETY: sharing the guard exposes only shared access to T.
unsafe impl<T: ?Sized + Sync> Sync for MutexGuard<'_, T> {}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the guard owns the lock until its destructor releases it.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard owns the lock and is exclusively borrowed.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        if !self.panicking && crate::thread::panicking() {
            self.lock.poisoned.store(1, Relaxed);
        }
        self.lock.locked.store(0, Release);
    }
}
