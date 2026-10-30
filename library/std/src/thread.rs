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
