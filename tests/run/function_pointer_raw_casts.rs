use std::sync::atomic::{AtomicPtr, Ordering};

fn identity<T: Copy>(value: T) -> T {
    value
}
fn increment(value: u32) -> u32 {
    value + 1
}
static DISPATCH: AtomicPtr<()> = AtomicPtr::new(increment as *mut ());
fn raw<T: Copy>() -> *const () {
    identity::<T> as *const ()
}

fn main() {
    let pointer: fn(u32) -> u32 = increment;
    assert_eq!(increment as *const (), pointer as *const ());
    assert_eq!(increment as *mut (), pointer as *mut ());
    assert_eq!(raw::<u32>(), identity::<u32> as *const ());
    assert_ne!(raw::<u64>(), std::ptr::null());
    // SAFETY: the stored address names `increment` with this exact signature.
    let call: fn(u32) -> u32 = unsafe { std::mem::transmute(DISPATCH.load(Ordering::Relaxed)) };
    println!("raw dispatch {} {}", call(41), pointer(6));
}
