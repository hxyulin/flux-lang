//! Owned panic payloads and catch boundaries over the platform unwinder.

pub use core::panic::*;
use core::any::Any;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::boxed::Box;
use crate::string::String;

/// The payload and source location supplied to a panic hook.
pub struct PanicHookInfo<'a> {
    payload: &'a (dyn Any + Send),
    location: &'a Location<'static>,
}
impl<'a> PanicHookInfo<'a> {
    pub fn payload(&self) -> &(dyn Any + Send) { self.payload }
    pub fn location(&self) -> Option<&Location<'_>> { Some(self.location) }
}
impl core::fmt::Display for PanicHookInfo<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "panicked at {}:\n{}", self.location, message(self.payload))
    }
}
#[deprecated(note = "use PanicHookInfo instead")]
pub type PanicInfo<'a> = PanicHookInfo<'a>;

type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static>;
static HOOK: crate::sync::RwLock<Option<Hook>> = crate::sync::RwLock::new(None);

fn message(payload: &(dyn Any + Send)) -> &str {
    if let Some(value) = payload.downcast_ref::<&'static str>() { value }
    else if let Some(value) = payload.downcast_ref::<String>() { value.as_str() }
    else { "Box<dyn Any>" }
}
fn default_hook(info: &PanicHookInfo<'_>) {
    let thread = crate::thread::try_current();
    let name = match &thread {
        Some(thread) => thread.name().unwrap_or("<unnamed>"),
        None => if crate::thread::is_main_thread() { "main" } else { "<unnamed>" },
    };
    core::panicking::__panic_print(format_args!("{}", message(info.payload)), info.location, name);
}

pub fn set_hook(hook: Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static>) {
    assert!(!crate::thread::panicking(), "cannot modify the panic hook from a panicking thread");
    let old = {
        let mut state = HOOK.write().unwrap();
        core::mem::replace(&mut *state, Some(hook))
    };
    drop(old);
}
pub fn take_hook() -> Hook {
    assert!(!crate::thread::panicking(), "cannot modify the panic hook from a panicking thread");
    let old = HOOK.write().unwrap().take();
    match old { Some(hook) => hook, None => Box::new(default_hook) }
}

// A key has no destructor. Its small integer value has no owned allocation and
// stays usable while other TLS destructors run. Competing key initializations
// delete their unused key instead of waiting on a possibly panicking initializer.
#[cfg(target_os = "macos")]
type Key = usize;
#[cfg(not(target_os = "macos"))]
type Key = u32;
static KEY: AtomicUsize = AtomicUsize::new(0);
// Most panic-state queries happen while acquiring ordinary locks. Avoid a TLS
// syscall when no thread has an active panic; own-thread increments precede
// every observable hook or cleanup operation.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" {
    fn pthread_key_create(key: *mut Key, destructor: Option<unsafe extern "C" fn(*mut u8)>) -> i32;
    fn pthread_key_delete(key: Key) -> i32;
    fn pthread_getspecific(key: Key) -> *mut u8;
    fn pthread_setspecific(key: Key, value: *const u8) -> i32;
    fn __gxx_personality_v0(version:i32, actions:i32, class:u64, exception:*mut u8, context:*mut u8) -> i32;
    fn _Unwind_DeleteException(exception: *mut Exception);
}
unsafe extern "C-unwind" { fn _Unwind_RaiseException(exception: *mut Exception) -> i32; }

fn abort() -> ! { core::panicking::abort_now() }
fn runtime_abort(message: &str) -> ! {
    core::io::_eprint(format_args!("fatal runtime error: {message}, aborting\n"));
    abort()
}
fn key() -> Key {
    let cached = KEY.load(Ordering::Acquire);
    if cached != 0 { return (cached - 1) as Key; }
    let mut key: Key = 0;
    // SAFETY: the key is process-owned and has no destruction callback.
    if unsafe { pthread_key_create(&mut key, None) } != 0 { abort(); }
    match KEY.compare_exchange(0, key as usize + 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => key,
        Err(existing) => {
            // SAFETY: this key was never published or used by another thread.
            if unsafe { pthread_key_delete(key) } != 0 { abort(); }
            (existing - 1) as Key
        }
    }
}
fn state() -> usize {
    // SAFETY: a process-owned key is queried only for the current thread.
    unsafe { pthread_getspecific(key()) as usize }
}
fn set_state(value: usize) {
    // SAFETY: this is an integer marker, never dereferenced or destroyed.
    if unsafe { pthread_setspecific(key(), value as *const u8) } != 0 { abort(); }
}
pub(crate) fn is_panicking() -> bool { ACTIVE.load(Ordering::Relaxed) != 0 && state() >> 1 != 0 }
fn enter() {
    let old = state();
    if old & 1 != 0 || old > usize::MAX - 2 { abort(); }
    set_state(old + 2);
    if ACTIVE.fetch_add(1, Ordering::Relaxed) == usize::MAX { abort(); }
}
fn leave() {
    let old = state();
    if old < 2 || old & 1 != 0 { abort(); }
    set_state(old - 2);
    if ACTIVE.fetch_sub(1, Ordering::Relaxed) == 0 { abort(); }
}

const CLASS: u64 = 0x464c555850414e00;
#[repr(C, align(16))]
struct Exception {
    class: u64,
    cleanup: unsafe extern "C" fn(i32, *mut Exception),
    private: [usize; 2],
    payload: ManuallyDrop<Box<dyn Any + Send>>,
    owned: bool,
    counted: bool,
}
unsafe extern "C" fn cleanup(_: i32, pointer: *mut Exception) {
    if core::panicking::__panic_unwind_enabled() {
    // SAFETY: the unwinder calls this once for our live exception allocation.
    let mut exception = unsafe { Box::from_raw(pointer) };
    // A Flux catch clears both flags before deleting its native header. A
    // foreign handler deleting an active panic must not silently consume it.
    // Keep the panic state active while destroying its payload, as rustc does.
    let foreign = exception.owned || exception.counted;
    if exception.owned {
        exception.owned = false;
        // SAFETY: ownership was not transferred to a catch result.
        unsafe { ManuallyDrop::drop(&mut exception.payload); }
    }
    drop(exception);
    if foreign { runtime_abort("Rust panics must be rethrown"); }
    } else { abort(); }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __flux_eh_personality(version:i32, actions:i32, class:u64, exception:*mut u8, context:*mut u8) -> i32 {
    if core::panicking::__panic_unwind_enabled() {
        // SAFETY: Keel uses the platform LSDA format; all arguments come from
        // the unwinder. Flux exceptions use a foreign exception class.
        unsafe { __gxx_personality_v0(version, actions, class, exception, context) }
    } else { 8 }
}

fn raise(payload: Box<dyn Any + Send>) -> ! {
    // Staged builtins compile private helpers too. Abort variants must not
    // retain references to the platform unwinder in those unused helpers.
    if core::panicking::__panic_unwind_enabled() {
    let exception = Box::into_raw(Box::new(Exception {
        class: CLASS, cleanup, private: [0; 2], payload: ManuallyDrop::new(payload), owned: true, counted: true,
    }));
    // SAFETY: the header has the native alignment and remains owned by the
    // unwinder until a handler consumes it or the cleanup callback deletes it.
    let _ = unsafe { _Unwind_RaiseException(exception) };
    // No handler, or an unwinder failure. Returning would violate the ! contract.
    abort()
    } else { core::mem::forget(payload); abort() }
}
fn start(payload: Box<dyn Any + Send>, location: &'static Location<'static>) -> ! {
    enter();
    start_counted(payload, location)
}
fn start_counted(payload: Box<dyn Any + Send>, location: &'static Location<'static>) -> ! {
    set_state(state() | 1);
    {
        let hook = HOOK.read().unwrap();
        let info = PanicHookInfo { payload: &*payload, location };
        match &*hook { Some(hook) => hook(&info), None => default_hook(&info) }
    }
    set_state(state() & !1);
    if core::panicking::__panic_unwind_enabled() { raise(payload) }
    else { core::mem::forget(payload); abort() }
}

#[track_caller]
pub fn panic_any<M: Any + Send>(message: M) -> ! { start(Box::new(message), Location::caller()) }

#[unsafe(no_mangle)]
extern "C-unwind" fn __flux_panic_str(data: *const u8, len: usize, location: *const Location<'static>) -> ! {
    // SAFETY: core supplies a static UTF-8 string and a static caller location.
    let message: &'static str = unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(data, len)) };
    // SAFETY: the compiler-generated location also has static storage.
    start(Box::new(message), unsafe { &*location })
}
#[unsafe(no_mangle)]
extern "C-unwind" fn __flux_panic_fmt(args: *const core::fmt::Arguments<'_>, location: *const Location<'static>) -> ! {
    // SAFETY: core keeps its Arguments alive until this call owns the message.
    let args = unsafe { *args };
    // SAFETY: core supplies a static compiler-generated caller location.
    let location = unsafe { &*location };
    match args.as_str() {
        Some(message) => start(Box::new(message), location),
        None => {
            // Formatting invokes user Display code while the original panic
            // is being prepared. Another panic here must terminate, not replace
            // the original payload and escape through its catch boundary.
            enter();
            set_state(state() | 1);
            let payload: Box<dyn Any + Send> = Box::new(crate::fmt::format(args));
            set_state(state() & !1);
            start_counted(payload, location)
        }
    }
}

/// Calls `f`, returning its value or the owned panic payload.
pub fn catch_unwind<F: FnOnce() -> R + UnwindSafe, R>(f: F) -> crate::thread::Result<R> {
    if core::panicking::__panic_unwind_enabled() {
        union Data<F, R> { function: ManuallyDrop<F>, result: ManuallyDrop<R> }
        unsafe extern "C-unwind" fn call<F: FnOnce() -> R, R>(data: *mut u8) {
            // SAFETY: catch_unwind supplies its live Data<F,R> and calls once.
            let data = unsafe { &mut *(data as *mut Data<F,R>) };
            // SAFETY: the callback owns the initialized function field.
            let function = unsafe { ManuallyDrop::take(&mut data.function) };
            let result = function();
            data.result = ManuallyDrop::new(result);
        }
        let mut data = Data { function: ManuallyDrop::new(f) };
        // SAFETY: the callback consumes F once and writes R only on success.
        let exception = unsafe { core::panicking::__panic_catch(call::<F,R>, &mut data as *mut Data<F,R> as *mut u8) } as *mut Exception;
        if exception.is_null() {
            // SAFETY: normal callback return initialized the result field.
            Ok(unsafe { ManuallyDrop::into_inner(data.result) })
        } else {
            // SAFETY: the catch primitive returned a live exception header.
            Err(unsafe { take_exception(exception) })
        }
    } else { Ok(f()) }
}

unsafe fn take_exception(exception: *mut Exception) -> Box<dyn Any + Send> {
    if core::panicking::__panic_unwind_enabled() {
    // SAFETY: the caller supplies a live native exception header.
    if unsafe { (*exception).class } != CLASS {
        // SAFETY: only the common unwinder header is inspected. Its cleanup
        // callback owns the foreign object and must run before termination.
        unsafe { _Unwind_DeleteException(exception); }
        runtime_abort("Rust cannot catch foreign exceptions");
    }
    // SAFETY: our class identifies the complete allocation; transfer its payload.
    let payload = unsafe {
        let payload = ManuallyDrop::take(&mut (*exception).payload);
        (*exception).owned = false;
        (*exception).counted = false;
        payload
    };
    leave();
    // SAFETY: cleanup now frees only the header, without dropping the payload.
    unsafe { _Unwind_DeleteException(exception); }
    payload
    } else { abort() }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __flux_panic_finish(exception: *mut Exception) -> i32 {
    // SAFETY: the compiler's process-root landing pad supplies this exception.
    drop(unsafe { take_exception(exception) });
    101
}

/// Restarts unwinding with `payload`, without running the panic hook again.
pub fn resume_unwind(payload: Box<dyn Any + Send>) -> ! {
    if core::panicking::__panic_unwind_enabled() { enter(); raise(payload) }
    else { core::mem::forget(payload); abort() }
}
