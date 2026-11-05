//! Panic-state queries and lazily initialized native thread-local storage.

use crate::time::Duration;

/// Flux panics abort the process without unwinding or running destructors.
/// Ordinary executing code therefore never observes an unwinding thread.
pub fn panicking() -> bool {
    false
}

mod local;
pub use local::{AccessError, LocalKey};

mod scoped;
pub use scoped::{scope, spawn, JoinHandle, Scope, ScopedJoinHandle};

/// A joined thread's result. Worker panics currently abort in Flux.
pub type Result<T> = core::result::Result<T, crate::boxed::Box<dyn core::any::Any + Send + 'static>>;

/// Sleeps for at least the requested duration, resuming after signal interruption.
pub fn sleep(duration: crate::time::Duration) {
    #[repr(C)]
    struct Timespec { seconds: i64, nanos: i64 }
    unsafe extern "C" { fn nanosleep(request: *const Timespec, remaining: *mut Timespec) -> i32; }
    let mut seconds = duration.as_secs();
    let mut nanos = duration.subsec_nanos() as i64;
    while seconds != 0 || nanos != 0 {
        let chunk = seconds.min(i64::MAX as u64);
        let request = Timespec { seconds: chunk as i64, nanos };
        let mut remaining = Timespec { seconds: 0, nanos: 0 };
        // SAFETY: both records have the native time_t/long layout and remain
        // live for this synchronous call; nanos is always below one second.
        if unsafe { nanosleep(&request, &mut remaining) } == 0 {
            seconds -= chunk;
            nanos = 0;
        } else {
            let error = crate::io::Error::last_os_error();
            assert_eq!(error.raw_os_error(), Some(4), "nanosleep failed");
            seconds = seconds - chunk + remaining.seconds as u64;
            nanos = remaining.nanos;
        }
    }
}

/// An estimate of the parallelism available to this process: the CPUs in its
/// affinity mask on Linux, and the online CPUs on macOS.
pub fn available_parallelism() -> crate::io::Result<core::num::NonZeroUsize> {
    unsafe extern "C" {
        fn sysconf(name: i32) -> core::ffi::c_long;
        #[cfg(target_os = "linux")]
        fn sched_getaffinity(pid: i32, size: usize, mask: *mut u64) -> i32;
    }
    #[cfg(target_os = "linux")]
    {
        // glibc's cpu_set_t: 1024 CPUs as 16 words.
        let mut mask = [0u64; 16];
        // SAFETY: mask is writable for exactly the size passed; pid 0 is this thread.
        if unsafe { sched_getaffinity(0, core::mem::size_of_val(&mask), mask.as_mut_ptr()) } == 0 {
            let count = mask.iter().map(|word| word.count_ones() as usize).sum::<usize>();
            if let Some(count) = core::num::NonZeroUsize::new(count) {
                return Ok(count);
            }
        }
    }
    #[cfg(target_os = "macos")]
    const ONLINE_PROCESSORS: i32 = 58;
    #[cfg(target_os = "linux")]
    const ONLINE_PROCESSORS: i32 = 84;
    // SAFETY: sysconf takes no pointers.
    let count = unsafe { sysconf(ONLINE_PROCESSORS) };
    match count {
        -1 => Err(crate::io::Error::last_os_error()),
        0 => Err(crate::io::Error::new(
            crate::io::ErrorKind::NotFound,
            "The number of hardware threads is not known for the target platform",
        )),
        count => Ok(core::num::NonZeroUsize::new(count as usize).expect("count is nonzero")),
    }
}

/// A process-unique thread identity that is not reused after a thread exits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(core::num::NonZeroU64);

static NEXT_THREAD_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
impl ThreadId {
    fn new() -> ThreadId {
        let id = NEXT_THREAD_ID
            .fetch_update(core::sync::atomic::Ordering::Relaxed, core::sync::atomic::Ordering::Relaxed, |id| id.checked_add(1))
            .expect("thread id space exhausted");
        ThreadId(core::num::NonZeroU64::new(id).unwrap())
    }
}

// Parker states, as in std's generic parker.
const EMPTY: u8 = 0;
const PARKED: u8 = 1;
const NOTIFIED: u8 = 2;

struct Inner {
    id: ThreadId,
    name: Option<crate::alloc_crate::string::String>,
    state: core::sync::atomic::AtomicU8,
    native: crate::sync::OnceLock<crate::sync::native::Native>,
}

/// A handle to a thread: its identity, name, and parking token.
#[derive(Clone)]
pub struct Thread { inner: crate::alloc_crate::sync::Arc<Inner> }
impl Thread {
    pub(crate) fn new(name: Option<crate::alloc_crate::string::String>) -> Thread {
        Thread { inner: crate::alloc_crate::sync::Arc::new(Inner {
            id: ThreadId::new(),
            name,
            state: core::sync::atomic::AtomicU8::new(EMPTY),
            native: crate::sync::OnceLock::new(),
        }) }
    }
    pub fn id(&self) -> ThreadId { self.inner.id }
    /// The name given at spawn, or "main" for the main thread.
    pub fn name(&self) -> Option<&str> { self.inner.name.as_deref() }
    fn native(&self) -> &crate::sync::native::Native { self.inner.native.get_or_init(crate::sync::native::Native::new) }
    /// Makes the thread's parking token available, waking it if it is parked.
    pub fn unpark(&self) {
        use core::sync::atomic::Ordering::Release;
        match self.inner.state.swap(NOTIFIED, Release) {
            EMPTY | NOTIFIED => return,
            _ => {}
        }
        // Taking the lock orders this notification after the parked thread
        // has started waiting.
        let native = self.native();
        drop(native.lock());
        native.notify_one();
    }
    fn park(&self, timeout: Option<Duration>) {
        use core::sync::atomic::Ordering::{Acquire, Relaxed};
        let state = &self.inner.state;
        if state.compare_exchange(NOTIFIED, EMPTY, Acquire, Relaxed).is_ok() { return; }
        let native = self.native();
        let mut locked = native.lock();
        match state.compare_exchange(EMPTY, PARKED, Relaxed, Relaxed) {
            Ok(_) => {}
            Err(NOTIFIED) => { state.swap(EMPTY, Acquire); return; }
            Err(_) => panic!("inconsistent park state"),
        }
        match timeout {
            None => loop {
                locked.wait(None);
                if state.compare_exchange(NOTIFIED, EMPTY, Acquire, Relaxed).is_ok() { return; }
            },
            Some(timeout) => {
                locked.wait(Some(timeout));
                match state.swap(EMPTY, Acquire) {
                    NOTIFIED | PARKED => {}
                    _ => panic!("inconsistent park_timeout state"),
                }
            }
        }
    }
}
impl core::fmt::Debug for Thread {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Thread").field("id", &self.id()).field("name", &self.name()).finish_non_exhaustive()
    }
}

fn is_main_thread() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" { fn pthread_main_np() -> i32; }
        // SAFETY: pthread_main_np takes no arguments.
        unsafe { pthread_main_np() != 0 }
    }
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" { fn getpid() -> i32; fn gettid() -> i32; }
        // SAFETY: neither call takes arguments.
        unsafe { getpid() == gettid() }
    }
}

crate::thread_local! {
    static CURRENT: core::cell::OnceCell<Thread> = const { core::cell::OnceCell::new() };
}
/// Records the handle a spawned worker reports as current.
pub(crate) fn set_current(thread: Thread) {
    CURRENT.with(|current| { let _ = current.set(thread); });
}
/// Returns a handle to the caller, including externally created threads.
pub fn current() -> Thread {
    CURRENT.with(|current| current.get_or_init(|| {
        Thread::new(if is_main_thread() { Some(crate::alloc_crate::string::String::from("main")) } else { None })
    }).clone())
}

/// Offers the rest of the caller's time slice to other threads.
pub fn yield_now() {
    unsafe extern "C" { fn sched_yield() -> i32; }
    // SAFETY: sched_yield takes no arguments.
    unsafe { sched_yield(); }
}

/// Blocks until the current thread's token is available, then consumes it.
/// Wakeups may be spurious.
pub fn park() { current().park(None) }
/// Like park, but returns after at most `duration`.
pub fn park_timeout(duration: Duration) { current().park(Some(duration)) }
#[deprecated(note = "replaced by `std::thread::park_timeout`")]
pub fn park_timeout_ms(ms: u32) { park_timeout(Duration::from_millis(ms as u64)) }
#[deprecated(note = "replaced by `std::thread::sleep`")]
pub fn sleep_ms(ms: u32) { sleep(Duration::from_millis(ms as u64)) }

/// Thread configuration: a name and a stack size.
#[derive(Debug)]
pub struct Builder {
    name: Option<crate::alloc_crate::string::String>,
    stack_size: Option<usize>,
}
impl Builder {
    pub fn new() -> Builder { Builder { name: None, stack_size: None } }
    pub fn name(mut self, name: crate::alloc_crate::string::String) -> Builder { self.name = Some(name); self }
    pub fn stack_size(mut self, size: usize) -> Builder { self.stack_size = Some(size); self }
    pub fn spawn<F, T>(self, f: F) -> crate::io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        scoped::spawn_with(self.name, self.stack_size, f)
    }
    pub fn spawn_scoped<'scope, 'env, F, T>(self, scope: &'scope Scope<'scope, 'env>, f: F) -> crate::io::Result<ScopedJoinHandle<'scope, T>>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        scope.spawn_with(self.name, self.stack_size, f)
    }
}
