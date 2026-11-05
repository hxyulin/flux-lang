//! Panic-state queries and lazily initialized native thread-local storage.

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThreadId(usize);

/// The currently supported thread handle operations identify the thread.
#[derive(Clone, Debug)]
pub struct Thread { id: ThreadId }
impl Thread { pub fn id(&self) -> ThreadId { self.id } }

static NEXT_THREAD_ID: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(1);
crate::thread_local! {
    static CURRENT_THREAD_ID: ThreadId = ThreadId(NEXT_THREAD_ID.fetch_update(
        core::sync::atomic::Ordering::Relaxed,
        core::sync::atomic::Ordering::Relaxed,
        |id| id.checked_add(1),
    ).expect("thread id space exhausted"));
}
/// Returns a handle identifying the caller, including externally created threads.
pub fn current() -> Thread {
    Thread { id: CURRENT_THREAD_ID.with(|id| *id) }
}
