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

impl<T: ?Sized + core::fmt::Debug> core::fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut debug = f.debug_struct("Mutex");
        if self.locked.compare_exchange(0, 1, Acquire, Relaxed).is_ok() {
            let guard = MutexGuard {
                lock: self,
                panicking: crate::thread::panicking(),
                marker: PhantomData,
            };
            debug.field("data", &&*guard);
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

impl<T: ?Sized> core::panic::UnwindSafe for Mutex<T> {}
impl<T: ?Sized> core::panic::RefUnwindSafe for Mutex<T> {}

impl<T: ?Sized> Mutex<T> {
    /// Acquires the lock only if no other guard holds it.
    pub fn try_lock(&self) -> super::TryLockResult<MutexGuard<'_, T>> {
        if self.locked.compare_exchange(0, 1, Acquire, Relaxed).is_err() {
            return Err(super::TryLockError::WouldBlock);
        }
        let guard = MutexGuard { lock: self, panicking: crate::thread::panicking(), marker: PhantomData };
        if self.is_poisoned() { Err(super::TryLockError::Poisoned(PoisonError::new(guard))) } else { Ok(guard) }
    }
}
impl<T> From<T> for Mutex<T> {
    fn from(value: T) -> Mutex<T> { Mutex::new(value) }
}
impl<T: ?Sized + core::fmt::Debug> core::fmt::Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { core::fmt::Debug::fmt(&**self, f) }
}
impl<T: ?Sized + core::fmt::Display> core::fmt::Display for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { core::fmt::Display::fmt(&**self, f) }
}

/// A native pthread mutex and condition variable, boxed so neither moves
/// after initialization. Condvar and thread parking block on it.
pub(crate) mod native {
    use crate::alloc_crate::boxed::Box;
    use crate::time::Duration;
    use core::cell::UnsafeCell;

    #[repr(C)]
    struct Timespec { secs: i64, nanos: i64 }
    unsafe extern "C" {
        fn pthread_mutex_init(mutex: *mut u64, attr: *const u8) -> i32;
        fn pthread_mutex_destroy(mutex: *mut u64) -> i32;
        fn pthread_mutex_lock(mutex: *mut u64) -> i32;
        fn pthread_mutex_unlock(mutex: *mut u64) -> i32;
        fn pthread_cond_init(cond: *mut u64, attr: *const u8) -> i32;
        fn pthread_cond_destroy(cond: *mut u64) -> i32;
        fn pthread_cond_wait(cond: *mut u64, mutex: *mut u64) -> i32;
        #[cfg(target_os = "macos")]
        fn pthread_cond_timedwait_relative_np(cond: *mut u64, mutex: *mut u64, time: *const Timespec) -> i32;
        #[cfg(target_os = "linux")]
        fn pthread_cond_clockwait(cond: *mut u64, mutex: *mut u64, clock: i32, time: *const Timespec) -> i32;
        #[cfg(target_os = "linux")]
        fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
        fn pthread_cond_broadcast(cond: *mut u64) -> i32;
        fn pthread_cond_signal(cond: *mut u64) -> i32;
        fn abort() -> !;
    }
    fn check(result: i32) {
        if result != 0 {
            // SAFETY: an invalid native synchronization state cannot be
            // recovered by continuing to use it.
            unsafe { abort() }
        }
    }
    // Darwin mutexes are 64 bytes, glibc's 40 on x86-64 and 48 on AArch64;
    // all condition variables are 48 bytes, aligned to eight.
    #[cfg(target_os = "macos")]
    const MUTEX_WORDS: usize = 8;
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const MUTEX_WORDS: usize = 5;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    const MUTEX_WORDS: usize = 6;
    struct Objects { mutex: UnsafeCell<[u64; MUTEX_WORDS]>, condition: UnsafeCell<[u64; 6]> }
    pub(crate) struct Native { objects: Box<Objects> }
    // SAFETY: the pthread objects are designed for use from any thread.
    unsafe impl Send for Native {}
    // SAFETY: every access goes through the pthread functions.
    unsafe impl Sync for Native {}
    impl Native {
        pub(crate) fn new() -> Native {
            let objects = Box::new(Objects { mutex: UnsafeCell::new([0; MUTEX_WORDS]), condition: UnsafeCell::new([0; 6]) });
            // SAFETY: both pointers name aligned, exclusive storage that
            // does not move after initialization.
            unsafe {
                check(pthread_mutex_init(objects.mutex.get() as *mut u64, core::ptr::null()));
                check(pthread_cond_init(objects.condition.get() as *mut u64, core::ptr::null()));
            }
            Native { objects }
        }
        fn mutex(&self) -> *mut u64 { self.objects.mutex.get() as *mut u64 }
        fn condition(&self) -> *mut u64 { self.objects.condition.get() as *mut u64 }
        /// Holds the native mutex until the returned guard drops.
        pub(crate) fn lock(&self) -> Locked<'_> {
            // SAFETY: the mutex is initialized and lives as long as self.
            unsafe { check(pthread_mutex_lock(self.mutex())) };
            Locked { native: self }
        }
        pub(crate) fn notify_one(&self) {
            // SAFETY: the condition is initialized and lives as long as self.
            unsafe { check(pthread_cond_signal(self.condition())) };
        }
        pub(crate) fn notify_all(&self) {
            // SAFETY: the condition is initialized and lives as long as self.
            unsafe { check(pthread_cond_broadcast(self.condition())) };
        }
    }
    impl Drop for Native {
        fn drop(&mut self) {
            // SAFETY: exclusive ownership proves no thread waits or holds the lock.
            unsafe {
                check(pthread_cond_destroy(self.condition()));
                check(pthread_mutex_destroy(self.mutex()));
            }
        }
    }
    pub(crate) struct Locked<'a> { native: &'a Native }
    impl Locked<'_> {
        /// Releases the mutex while waiting; returns false after a timeout.
        /// Wakeups may be spurious.
        pub(crate) fn wait(&mut self, timeout: Option<Duration>) -> bool {
            let native = self.native;
            // SAFETY: this guard holds the mutex, which the wait releases
            // and reacquires before returning.
            unsafe {
                let Some(time) = timeout else {
                    check(pthread_cond_wait(native.condition(), native.mutex()));
                    return true;
                };
                let relative = Timespec { secs: core::cmp::min(time.as_secs(), 86_400) as i64, nanos: time.subsec_nanos() as i64 };
                #[cfg(target_os = "macos")]
                let result = pthread_cond_timedwait_relative_np(native.condition(), native.mutex(), &relative);
                #[cfg(target_os = "linux")]
                let result = {
                    let mut now = Timespec { secs: 0, nanos: 0 };
                    check(clock_gettime(1, &mut now)); // CLOCK_MONOTONIC.
                    let nanos = now.nanos + relative.nanos;
                    let deadline = Timespec { secs: now.secs + relative.secs + nanos / 1_000_000_000, nanos: nanos % 1_000_000_000 };
                    pthread_cond_clockwait(native.condition(), native.mutex(), 1, &deadline)
                };
                #[cfg(target_os = "macos")]
                let timed_out = 60;
                #[cfg(target_os = "linux")]
                let timed_out = 110;
                if result == timed_out { return false; }
                check(result);
                true
            }
        }
    }
    impl Drop for Locked<'_> {
        fn drop(&mut self) {
            // SAFETY: this guard owns one successful acquisition.
            unsafe { check(pthread_mutex_unlock(self.native.mutex())) };
        }
    }
}

/// Whether a timed condition-variable wait returned because time ran out.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct WaitTimeoutResult(bool);
impl WaitTimeoutResult {
    pub fn timed_out(&self) -> bool { self.0 }
}

/// Blocks threads until another thread notifies it. Waits may wake
/// spuriously, as std's may.
pub struct Condvar {
    // Created on first use, so that new can be const.
    native: crate::sync::OnceLock<native::Native>,
    generation: core::cell::UnsafeCell<u64>,
}
// SAFETY: generation is accessed only while the native mutex is held.
unsafe impl Send for Condvar {}
// SAFETY: generation is accessed only while the native mutex is held.
unsafe impl Sync for Condvar {}

impl Condvar {
    pub const fn new() -> Condvar {
        Condvar { native: crate::sync::OnceLock::new(), generation: core::cell::UnsafeCell::new(0) }
    }
    fn native(&self) -> &native::Native { self.native.get_or_init(native::Native::new) }
    // Releases the guard's lock and waits for a notification after it.
    // Taking the native lock before releasing the user's lock means a
    // notifier that acquires the user's lock afterwards cannot be missed.
    fn wait_inner<'a, T: ?Sized>(&self, guard: MutexGuard<'a, T>, timeout: Option<crate::time::Duration>) -> (LockResult<MutexGuard<'a, T>>, bool) {
        let mutex = guard.lock;
        let native = self.native();
        let mut locked = native.lock();
        // SAFETY: the native mutex is held.
        let generation = unsafe { *self.generation.get() };
        drop(guard);
        let mut notified = true;
        // SAFETY: the native mutex is held whenever generation is read.
        if unsafe { *self.generation.get() } == generation {
            notified = locked.wait(timeout);
        }
        drop(locked);
        (mutex.lock(), notified)
    }
    pub fn wait<'a, T: ?Sized>(&self, guard: MutexGuard<'a, T>) -> LockResult<MutexGuard<'a, T>> {
        self.wait_inner(guard, None).0
    }
    pub fn wait_while<'a, T, F: FnMut(&mut T) -> bool>(&self, mut guard: MutexGuard<'a, T>, mut condition: F) -> LockResult<MutexGuard<'a, T>> {
        while condition(&mut *guard) {
            guard = self.wait(guard)?;
        }
        Ok(guard)
    }
    pub fn wait_timeout<'a, T: ?Sized>(&self, guard: MutexGuard<'a, T>, duration: crate::time::Duration) -> LockResult<(MutexGuard<'a, T>, WaitTimeoutResult)> {
        let (result, notified) = self.wait_inner(guard, Some(duration));
        match result {
            Ok(guard) => Ok((guard, WaitTimeoutResult(!notified))),
            Err(error) => Err(PoisonError::new((error.into_inner(), WaitTimeoutResult(!notified)))),
        }
    }
    #[deprecated(note = "replaced by `std::sync::Condvar::wait_timeout`")]
    pub fn wait_timeout_ms<'a, T: ?Sized>(&self, guard: MutexGuard<'a, T>, ms: u32) -> LockResult<(MutexGuard<'a, T>, bool)> {
        match self.wait_timeout(guard, crate::time::Duration::from_millis(ms as u64)) {
            Ok((guard, result)) => Ok((guard, !result.timed_out())),
            Err(error) => { let (guard, result) = error.into_inner(); Err(PoisonError::new((guard, !result.timed_out()))) }
        }
    }
    pub fn wait_timeout_while<'a, T, F: FnMut(&mut T) -> bool>(&self, mut guard: MutexGuard<'a, T>, duration: crate::time::Duration, mut condition: F) -> LockResult<(MutexGuard<'a, T>, WaitTimeoutResult)> {
        let start = crate::time::Instant::now();
        loop {
            if !condition(&mut *guard) { return Ok((guard, WaitTimeoutResult(false))); }
            let timeout = match duration.checked_sub(start.elapsed()) {
                Some(timeout) => timeout,
                None => return Ok((guard, WaitTimeoutResult(true))),
            };
            guard = self.wait_timeout(guard, timeout)?.0;
        }
    }
    pub fn notify_one(&self) {
        let native = self.native();
        let locked = native.lock();
        // SAFETY: the native mutex is held.
        unsafe { *self.generation.get() += 1 };
        native.notify_one();
        drop(locked);
    }
    pub fn notify_all(&self) {
        let native = self.native();
        let locked = native.lock();
        // SAFETY: the native mutex is held.
        unsafe { *self.generation.get() += 1 };
        native.notify_all();
        drop(locked);
    }
}
impl Default for Condvar {
    fn default() -> Condvar { Condvar::new() }
}
impl core::fmt::Debug for Condvar {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("Condvar").finish_non_exhaustive() }
}
