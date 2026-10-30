//! Native blocking channels on macOS and glibc x86-64 Linux.
use crate::alloc_crate::{boxed::Box, collections::VecDeque, sync::Arc};
use crate::time::{Duration, Instant};
use core::cell::{Cell, UnsafeCell};
use core::fmt::{self, Debug, Display, Formatter};
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

#[repr(C)]
struct Timespec {
    secs: i64,
    nanos: i64,
}
unsafe extern "C" {
    fn pthread_mutex_init(mutex: *mut u64, attr: *const u8) -> i32;
    fn pthread_mutex_destroy(mutex: *mut u64) -> i32;
    fn pthread_mutex_lock(mutex: *mut u64) -> i32;
    fn pthread_mutex_unlock(mutex: *mut u64) -> i32;
    fn pthread_cond_init(cond: *mut u64, attr: *const u8) -> i32;
    fn pthread_cond_destroy(cond: *mut u64) -> i32;
    fn pthread_cond_wait(cond: *mut u64, mutex: *mut u64) -> i32;
    #[cfg(target_os = "macos")]
    fn pthread_cond_timedwait_relative_np(
        cond: *mut u64,
        mutex: *mut u64,
        time: *const Timespec,
    ) -> i32;
    #[cfg(target_os = "linux")]
    fn pthread_cond_clockwait(cond: *mut u64, mutex: *mut u64, clock: i32, time: *const Timespec) -> i32;
    #[cfg(target_os = "linux")]
    fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
    fn pthread_cond_broadcast(cond: *mut u64) -> i32;
    fn abort() -> !;
}
fn check(result: i32) {
    if result != 0 {
        // SAFETY: an invalid native synchronization state cannot be recovered
        // by releasing storage or continuing to access the channel state.
        unsafe { abort() }
    }
}

// Darwin mutexes are 64 bytes; glibc x86-64 mutexes are 40. Both condition
// variables are 48 bytes, aligned to eight (native pthread_types headers).
// The box pins initialized objects throughout every native wait and lock.
#[cfg(target_os = "macos")]
const MUTEX_WORDS: usize = 8;
#[cfg(target_os = "linux")]
const MUTEX_WORDS: usize = 5;
struct Native {
    mutex: UnsafeCell<[u64; MUTEX_WORDS]>,
    condition: UnsafeCell<[u64; 6]>,
}
impl Native {
    fn new() -> Box<Self> {
        let native = Box::new(Self {
            mutex: UnsafeCell::new([0; MUTEX_WORDS]),
            condition: UnsafeCell::new([0; 6]),
        });
        // SAFETY: both pointers name aligned, exclusive, uninitialized native
        // objects. Neither object moves after pthread initialization.
        unsafe {
            check(pthread_mutex_init(
                native.mutex.get() as *mut u64,
                core::ptr::null(),
            ));
            check(pthread_cond_init(
                native.condition.get() as *mut u64,
                core::ptr::null(),
            ));
        }
        native
    }
    fn notify(&self) {
        // SAFETY: the condition is initialized and the current Arc keeps it alive.
        unsafe {
            check(pthread_cond_broadcast(self.condition.get() as *mut u64));
        }
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        // SAFETY: the final channel Arc can drop only after every endpoint and
        // active operation has gone; no thread still waits or holds this lock.
        unsafe {
            check(pthread_cond_destroy(self.condition.get() as *mut u64));
            check(pthread_mutex_destroy(self.mutex.get() as *mut u64));
        }
    }
}

struct State<T> {
    queue: VecDeque<T>,
    senders: usize,
    receiver: bool,
    waiting: bool,
    pending: Option<(u64, T)>,
    next: u64,
    received: u64,
}
struct Shared<T> {
    native: Box<Native>,
    capacity: Option<usize>,
    state: UnsafeCell<State<T>>,
}
// SAFETY: queued values cross thread boundaries only when T is Send. Native
// locking serializes every access, even when producers share their endpoints.
unsafe impl<T: Send> Send for Shared<T> {}
// SAFETY: no reference to a queued T escapes the native mutex guard.
unsafe impl<T: Send> Sync for Shared<T> {}
struct Guard<'a, T> {
    shared: &'a Shared<T>,
}
impl<T> Deref for Guard<'_, T> {
    type Target = State<T>;
    fn deref(&self) -> &State<T> {
        // SAFETY: Guard owns the native lock until its Drop releases it.
        unsafe { &*self.shared.state.get() }
    }
}
impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut State<T> {
        // SAFETY: one guard owns the lock, so this mutable borrow is exclusive.
        unsafe { &mut *self.shared.state.get() }
    }
}
impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        // SAFETY: this guard owns exactly one successful native acquisition.
        unsafe {
            check(pthread_mutex_unlock(
                self.shared.native.mutex.get() as *mut u64
            ));
        }
    }
}
impl<T> Guard<'_, T> {
    fn wait(&mut self, timeout: Option<Duration>) {
        let native = &self.shared.native;
        // SAFETY: the mutex is held on entry and atomically released while
        // waiting, then reacquired before return. No State borrow spans wait.
        unsafe {
            if let Some(time) = timeout {
                let secs = core::cmp::min(time.as_secs(), 86_400) as i64;
                let relative = Timespec {
                    secs,
                    nanos: time.subsec_nanos() as i64,
                };
                #[cfg(target_os = "macos")]
                let result = pthread_cond_timedwait_relative_np(
                    native.condition.get() as *mut u64,
                    native.mutex.get() as *mut u64,
                    &relative,
                );
                #[cfg(target_os = "linux")]
                let result = {
                    let mut now = Timespec { secs: 0, nanos: 0 };
                    check(clock_gettime(1, &mut now)); // CLOCK_MONOTONIC.
                    let nanos = now.nanos + relative.nanos;
                    let deadline = Timespec { secs: now.secs + relative.secs + nanos / 1_000_000_000, nanos: nanos % 1_000_000_000 };
                    pthread_cond_clockwait(native.condition.get() as *mut u64, native.mutex.get() as *mut u64, 1, &deadline)
                };
                #[cfg(target_os = "macos")]
                let timed_out = 60;
                #[cfg(target_os = "linux")]
                let timed_out = 110;
                if result != timed_out { check(result); }
            } else {
                check(pthread_cond_wait(
                    native.condition.get() as *mut u64,
                    native.mutex.get() as *mut u64,
                ));
            }
        }
    }
}
impl<T> Shared<T> {
    fn lock(&self) -> Guard<'_, T> {
        // SAFETY: Native remains initialized for this shared borrow's lifetime.
        unsafe {
            check(pthread_mutex_lock(self.native.mutex.get() as *mut u64));
        }
        Guard { shared: self }
    }
    fn clone_sender(&self) {
        let mut state = self.lock();
        state.senders = state
            .senders
            .checked_add(1)
            .expect("too many channel senders");
    }
    fn drop_sender(&self) {
        let mut state = self.lock();
        state.senders -= 1;
        if state.senders == 0 {
            self.native.notify();
        }
    }
    fn take(&self, state: &mut State<T>) -> Option<T> {
        if self.capacity == Some(0) {
            if let Some((id, value)) = state.pending.take() {
                state.received = id;
                self.native.notify();
                Some(value)
            } else {
                None
            }
        } else {
            let value = state.queue.pop_front();
            if value.is_some() {
                self.native.notify();
            }
            value
        }
    }
}

pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}
pub struct SyncSender<T> {
    shared: Arc<Shared<T>>,
}
/// Exactly one consumer may access the receive end at a time.
pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
    marker: PhantomData<Cell<()>>,
}

// Queue transitions never invoke user code. Guards restore native lock state
// on unwind, and payload destructors run only after unlocking.
impl<T> core::panic::UnwindSafe for Sender<T> {}
impl<T> core::panic::RefUnwindSafe for Sender<T> {}
impl<T> core::panic::UnwindSafe for SyncSender<T> {}
impl<T> core::panic::RefUnwindSafe for SyncSender<T> {}
impl<T> core::panic::UnwindSafe for Receiver<T> {}
impl<T> core::panic::RefUnwindSafe for Receiver<T> {}

fn create<T>(capacity: Option<usize>) -> (Arc<Shared<T>>, Receiver<T>) {
    let shared = Arc::new(Shared {
        native: Native::new(),
        capacity,
        state: UnsafeCell::new(State {
            queue: VecDeque::new(),
            senders: 1,
            receiver: true,
            waiting: false,
            pending: None,
            next: 0,
            received: 0,
        }),
    });
    let receiver = Receiver {
        shared: shared.clone(),
        marker: PhantomData,
    };
    (shared, receiver)
}
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (shared, receiver) = create(None);
    (Sender { shared }, receiver)
}
pub fn sync_channel<T>(bound: usize) -> (SyncSender<T>, Receiver<T>) {
    let (shared, receiver) = create(Some(bound));
    (SyncSender { shared }, receiver)
}
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared.clone_sender();
        Self {
            shared: self.shared.clone(),
        }
    }
}
impl<T> Clone for SyncSender<T> {
    fn clone(&self) -> Self {
        self.shared.clone_sender();
        Self {
            shared: self.shared.clone(),
        }
    }
}
impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.shared.drop_sender();
    }
}
impl<T> Drop for SyncSender<T> {
    fn drop(&mut self) {
        self.shared.drop_sender();
    }
}
impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.receiver = false;
        let queue = core::mem::take(&mut state.queue);
        self.shared.native.notify();
        drop(state);
        // User destructors must not run while the native mutex is locked.
        drop(queue);
    }
}
impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut state = self.shared.lock();
        if !state.receiver {
            return Err(SendError(value));
        }
        // Keep ownership here until allocation succeeds: an unwinding reserve
        // releases the guard before the argument's destructor can reenter.
        state.queue.reserve(1);
        state.queue.push_back(value);
        self.shared.native.notify();
        Ok(())
    }
}
impl<T> SyncSender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut state = self.shared.lock();
        let capacity = self.shared.capacity.unwrap();
        if capacity == 0 {
            while state.receiver && state.pending.is_some() {
                state.wait(None);
            }
            if !state.receiver {
                return Err(SendError(value));
            }
            let id = state
                .next
                .checked_add(1)
                .expect("too many rendezvous messages");
            state.next = id;
            state.pending = Some((id, value));
            self.shared.native.notify();
            while state.receiver && state.received < id {
                state.wait(None);
            }
            if state.received >= id {
                Ok(())
            } else {
                Err(SendError(state.pending.take().unwrap().1))
            }
        } else {
            while state.receiver && state.queue.len() == capacity {
                state.wait(None);
            }
            if !state.receiver {
                return Err(SendError(value));
            }
            state.queue.reserve(1);
            state.queue.push_back(value);
            self.shared.native.notify();
            Ok(())
        }
    }
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        let mut state = self.shared.lock();
        if !state.receiver {
            return Err(TrySendError::Disconnected(value));
        }
        let capacity = self.shared.capacity.unwrap();
        if capacity == 0 {
            if !state.waiting || state.pending.is_some() {
                return Err(TrySendError::Full(value));
            }
            let id = state
                .next
                .checked_add(1)
                .expect("too many rendezvous messages");
            state.next = id;
            state.pending = Some((id, value));
        } else {
            if state.queue.len() == capacity {
                return Err(TrySendError::Full(value));
            }
            state.queue.reserve(1);
            state.queue.push_back(value);
        }
        self.shared.native.notify();
        Ok(())
    }
}
impl<T> Receiver<T> {
    pub fn recv(&self) -> Result<T, RecvError> {
        let mut state = self.shared.lock();
        loop {
            if let Some(value) = self.shared.take(&mut state) {
                return Ok(value);
            }
            if state.senders == 0 {
                return Err(RecvError);
            }
            state.waiting = true;
            state.wait(None);
            state.waiting = false;
        }
    }
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut state = self.shared.lock();
        if let Some(value) = self.shared.take(&mut state) {
            return Ok(value);
        }
        if state.senders == 0 {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        let deadline = Instant::now().checked_add(timeout);
        let mut state = self.shared.lock();
        loop {
            if let Some(value) = self.shared.take(&mut state) {
                return Ok(value);
            }
            if state.senders == 0 {
                return Err(RecvTimeoutError::Disconnected);
            }
            let remaining = if let Some(deadline) = deadline {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO);
                if remaining == Duration::ZERO {
                    return Err(RecvTimeoutError::Timeout);
                }
                Some(remaining)
            } else {
                None
            };
            state.waiting = true;
            state.wait(remaining);
            state.waiting = false;
        }
    }
    pub fn iter(&self) -> Iter<'_, T> {
        Iter { receiver: self }
    }
    pub fn try_iter(&self) -> TryIter<'_, T> {
        TryIter { receiver: self }
    }
}
pub struct Iter<'a, T> {
    receiver: &'a Receiver<T>,
}
pub struct TryIter<'a, T> {
    receiver: &'a Receiver<T>,
}
pub struct IntoIter<T> {
    receiver: Receiver<T>,
}
impl<T> Iterator for Iter<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.receiver.recv().ok()
    }
}
impl<T> Iterator for TryIter<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.receiver.try_recv().ok()
    }
}
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.receiver.recv().ok()
    }
}
impl<T> IntoIterator for Receiver<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> IntoIter<T> {
        IntoIter { receiver: self }
    }
}
impl<'a, T> IntoIterator for &'a Receiver<T> {
    type Item = T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SendError<T>(pub T);
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct RecvError;
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TrySendError<T> {
    Full(T),
    Disconnected(T),
}
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RecvTimeoutError {
    Timeout,
    Disconnected,
}
impl<T> Debug for SendError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("SendError { .. }")
    }
}
impl Debug for RecvError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("RecvError")
    }
}
impl<T> Debug for TrySendError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(_) => f.write_str("TrySendError::Full(..)"),
            Self::Disconnected(_) => f.write_str("TrySendError::Disconnected(..)"),
        }
    }
}
impl<T> Debug for Sender<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Sender { .. }")
    }
}
impl<T> Debug for SyncSender<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("SyncSender { .. }")
    }
}
impl<T> Debug for Receiver<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Receiver { .. }")
    }
}
impl<T> Display for SendError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("sending on a closed channel")
    }
}
impl Display for RecvError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("receiving on a closed channel")
    }
}
impl Display for TryRecvError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "receiving on an empty channel",
            Self::Disconnected => "receiving on a closed channel",
        })
    }
}
impl<T> Display for TrySendError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Full(_) => "sending on a full channel",
            Self::Disconnected(_) => "sending on a closed channel",
        })
    }
}
impl Display for RecvTimeoutError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Timeout => "timed out waiting on channel",
            Self::Disconnected => "channel is empty and sending half is closed",
        })
    }
}
impl<T> core::error::Error for SendError<T> {}
impl core::error::Error for RecvError {}
impl core::error::Error for TryRecvError {}
impl<T> core::error::Error for TrySendError<T> {}
impl core::error::Error for RecvTimeoutError {}
impl<T> From<SendError<T>> for TrySendError<T> {
    fn from(error: SendError<T>) -> Self {
        Self::Disconnected(error.0)
    }
}
impl From<RecvError> for TryRecvError {
    fn from(_: RecvError) -> Self {
        Self::Disconnected
    }
}
impl From<RecvError> for RecvTimeoutError {
    fn from(_: RecvError) -> Self {
        Self::Disconnected
    }
}
