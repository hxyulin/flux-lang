use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicUsize, Ordering};

static INITIALIZED: AtomicUsize = AtomicUsize::new(0);
static DROPPED: AtomicUsize = AtomicUsize::new(0);
thread_local! {
    static VALUE: Cell<usize> = const { Cell::new(0) };
    static NUMBERS: RefCell<Vec<usize>> = RefCell::new(Vec::new());
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
#[cfg(target_os = "macos")]
type NativeThread = *mut u8;
#[cfg(target_os = "linux")]
type NativeThread = usize;
unsafe extern "C" {
    fn pthread_create(thread: *mut NativeThread, attr: *const u8, callback: extern "C" fn(*mut u8) -> *mut u8, arg: *mut u8) -> i32;
    fn pthread_join(thread: NativeThread, result: *mut *mut u8) -> i32;
}
extern "C" fn worker(arg: *mut u8) -> *mut u8 {
    NUMBERS.with_borrow(|numbers| assert!(numbers.is_empty()));
    let length = NUMBERS.with_borrow_mut(|numbers| { numbers.push(7); numbers.len() });
    assert_eq!(length, 1);
    NUMBERS.with_borrow(|numbers| NUMBERS.with_borrow(|again| assert_eq!(numbers, again)));
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
    NUMBERS.with_borrow_mut(|numbers| numbers.push(42));
    VALUE.with(|v| v.set(37));
    #[cfg(target_os = "macos")]
    let mut threads = [std::ptr::null_mut(); 8];
    #[cfg(target_os = "linux")]
    let mut threads = [0usize; 8];
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
    NUMBERS.with_borrow(|numbers| assert_eq!(numbers.as_slice(), &[42]));
    println!("tls 8 8 37 19");
}
