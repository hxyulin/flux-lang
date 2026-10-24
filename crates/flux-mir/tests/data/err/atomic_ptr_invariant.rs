use core::sync::atomic::{AtomicPtr, Ordering};

fn shorten<'a>(p: AtomicPtr<&'static u8>, _: &'a u8) -> AtomicPtr<&'a u8> {
    p
}

fn extend<'a>(p: *mut &'a u8) -> &'static u8 {
    // SAFETY: compile-only negative fixture; never executed.
    unsafe { *AtomicPtr::new(p).load(Ordering::Acquire) }
}

fn shared_mut(a: &AtomicPtr<u8>) -> &mut *mut u8 {
    a.get_mut()
}

fn main() {}
