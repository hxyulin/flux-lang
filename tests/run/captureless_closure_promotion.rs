use std::sync::atomic::{AtomicUsize, Ordering};
static CALLS: AtomicUsize = AtomicUsize::new(0);
fn callback() -> &'static dyn Fn(u32) -> u32 {
    &|value| { CALLS.fetch_add(1, Ordering::SeqCst); value + 7 }
}
fn pair() -> &'static dyn Fn(u32, u32) -> u32 { &|a,b| a+b }
fn main() {
    let f = callback();
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(f(5), 12);
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(pair()(11,29), 40);
    println!("{} {}", f(8), CALLS.load(Ordering::SeqCst));
}
