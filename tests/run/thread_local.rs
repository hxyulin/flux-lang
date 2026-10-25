use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

static INITIALIZED: AtomicUsize = AtomicUsize::new(0);
static DROPPED: AtomicUsize = AtomicUsize::new(0);
thread_local! {
    static VALUE: Cell<usize> = const { Cell::new(0) };
    static GUARD: Guard = {
        INITIALIZED.fetch_add(1, Ordering::SeqCst);
        Guard
    };
}
struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        assert!(GUARD.try_with(|_| ()).is_err());
        DROPPED.fetch_add(1, Ordering::SeqCst);
    }
}
unsafe extern "C" {
    fn pthread_create(thread: *mut *mut u8, attr: *const u8, callback: extern "C" fn(*mut u8) -> *mut u8, arg: *mut u8) -> i32;
    fn pthread_join(thread: *mut u8, result: *mut *mut u8) -> i32;
}
extern "C" fn worker(arg: *mut u8) -> *mut u8 {
    VALUE.with(|v| assert_eq!(v.get(), 0));
    for _ in 0..5 {
        VALUE.with(|v| v.set(v.get() + 1));
    }
    VALUE.with(|v| assert_eq!(v.get(), 5));
    GUARD.with(|_| ());
    GUARD.with(|_| ());
    arg
}
fn local_declaration() {
    thread_local! { static LOCAL: Cell<usize> = Cell::new(11); }
    LOCAL.with(|v| { assert_eq!(v.get(), 11); v.set(19); });
    LOCAL.with(|v| assert_eq!(v.get(), 19));
}
fn main() {
    VALUE.with(|v| v.set(37));
    let mut threads = [std::ptr::null_mut(); 8];
    for thread in &mut threads {
        // SAFETY: the callback borrows no caller data and accesses only atomic or TLS storage.
        assert_eq!(unsafe { pthread_create(thread, std::ptr::null(), worker, std::ptr::null_mut()) }, 0);
    }
    for thread in threads {
        // SAFETY: every successfully created native thread is joined exactly once.
        assert_eq!(unsafe { pthread_join(thread, std::ptr::null_mut()) }, 0);
    }
    VALUE.with(|v| assert_eq!(v.get(), 37));
    assert_eq!(INITIALIZED.load(Ordering::SeqCst), 8);
    assert_eq!(DROPPED.load(Ordering::SeqCst), 8);
    local_declaration();
    println!("tls 8 8 37 19");
}
