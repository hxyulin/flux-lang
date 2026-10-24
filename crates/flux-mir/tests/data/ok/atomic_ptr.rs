use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

static SLOT: AtomicPtr<&'static str> = AtomicPtr::new(ptr::null_mut());
static TEXT: &str = "pointer payload";

fn marker<T: Send + Sync>() {}
fn main() {
    marker::<AtomicPtr<core::cell::Cell<u8>>>();
    marker::<AtomicPtr<*mut u8>>();
    SLOT.store(ptr::from_ref(&TEXT).cast_mut(), Ordering::Release);
    let p = SLOT.load(Ordering::Acquire);
    let mut atomic = AtomicPtr::new(p);
    *atomic.get_mut() = ptr::null_mut();
    let _ = atomic.into_inner();
}
