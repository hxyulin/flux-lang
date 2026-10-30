//! Compiler entry-point handoff for Unix process arguments.

use crate::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

static ARGC: AtomicUsize = AtomicUsize::new(0);
static ARGV: AtomicPtr<*const u8> = AtomicPtr::new(crate::ptr::null_mut());

/// Publishes the argument array supplied by the C runtime.
///
/// # Safety
/// Call once, before user code starts, with a nonnegative `argc`.
/// `argv` must contain `argc` pointers to
/// immutable NUL-terminated strings that remain valid for the process lifetime.
#[lang = "args_init"]
pub unsafe fn init_args(argc: i32, argv: *mut *const u8) {
    ARGC.store(argc as usize, Ordering::Relaxed);
    ARGV.store(argv, Ordering::Release);
}

/// Borrows the process-lifetime argument array installed by the entry point.
pub fn args() -> (usize, *mut *const u8) {
    let argv = ARGV.load(Ordering::Acquire);
    (ARGC.load(Ordering::Relaxed), argv)
}
