//! A spin-waiting reader/writer lock with no fairness guarantee.

use core::cell::UnsafeCell;
use core::default::Default;
use core::marker::{PhantomData, Send, Sync};
use core::ops::{Deref, DerefMut, Drop};
use core::result::Result::{Err, Ok};
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use super::{LockResult, PoisonError};

const WRITE: usize = usize::MAX;

pub struct RwLock<T: ?Sized> {
    state: AtomicUsize,
    poisoned: AtomicUsize,
    value: UnsafeCell<T>,
}

// SAFETY: moving the lock transfers ownership of T.
unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
// SAFETY: the state permits either shared reads or one exclusive writer.
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    pub const fn new(value: T) -> RwLock<T> {
        RwLock {
            state: AtomicUsize::new(0),
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

impl<T: ?Sized> RwLock<T> {
    pub fn read(&self) -> LockResult<RwLockReadGuard<'_, T>> {
        loop {
            let state = self.state.load(Relaxed);
            if state < WRITE - 1 && self.state.compare_exchange(state, state + 1, Acquire, Relaxed).is_ok() {
                let guard = RwLockReadGuard { lock: self, marker: PhantomData };
                return if self.is_poisoned() { Err(PoisonError::new(guard)) } else { Ok(guard) };
            }
        }
    }

    pub fn write(&self) -> LockResult<RwLockWriteGuard<'_, T>> {
        while self.state.compare_exchange(0, WRITE, Acquire, Relaxed).is_err() {}
        let guard = RwLockWriteGuard {
            lock: self,
            panicking: crate::thread::panicking(),
            marker: PhantomData,
        };
        if self.is_poisoned() { Err(PoisonError::new(guard)) } else { Ok(guard) }
    }

    pub fn is_poisoned(&self) -> bool { self.poisoned.load(Relaxed) != 0 }

    pub fn clear_poison(&self) { self.poisoned.store(0, Relaxed); }

    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        if self.is_poisoned() {
            Err(PoisonError::new(self.value.get_mut()))
        } else {
            Ok(self.value.get_mut())
        }
    }
}

impl<T: Default> Default for RwLock<T> {
    fn default() -> RwLock<T> { RwLock::new(T::default()) }
}

pub struct RwLockReadGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
    marker: PhantomData<*mut ()>,
}

pub struct RwLockWriteGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
    panicking: bool,
    marker: PhantomData<*mut ()>,
}

// SAFETY: sharing either guard exposes only shared access to T.
unsafe impl<T: ?Sized + Sync> Sync for RwLockReadGuard<'_, T> {}
// SAFETY: sharing the write guard cannot mutate T without an exclusive borrow.
unsafe impl<T: ?Sized + Sync> Sync for RwLockWriteGuard<'_, T> {}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the live reader count prevents exclusive access.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: this guard holds the exclusive writer state.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: this guard holds the writer state and is exclusively borrowed.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) { self.lock.state.fetch_sub(1, Release); }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        if !self.panicking && crate::thread::panicking() {
            self.lock.poisoned.store(1, Relaxed);
        }
        self.lock.state.store(0, Release);
    }
}
