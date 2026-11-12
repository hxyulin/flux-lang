use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Owned(u32);
impl Drop for Owned { fn drop(&mut self) { DROPS.fetch_add(1, Ordering::Relaxed); } }
const fn double(x: u32) -> u32 { x * 2 }
fn values() -> &'static [u32] { const { &[double(7), double(11)] } }
fn generic<T>() -> &'static [u32] { const { &[double(13)] } }
fn nested() -> u32 {
    const { let mut values = [double(2), const { double(3) }]; values[0] += 1; values[0] + values[1] }
}
fn generic_value<const N: usize>() -> usize { const { N + 1 } }
fn main() {
    assert_eq!(values(), &[14, 22]);
    assert_eq!(generic::<u8>(), &[26]);
    assert_eq!(generic::<String>(), &[26]);
    assert_eq!(nested(), 11);
    assert_eq!(generic_value::<17>(), 18);
    let closure = || const { &[double(19)] };
    assert_eq!(closure(), &[38]);
    let owned = const { Owned(double(23)) };
    assert_eq!(owned.0, 46); drop(owned);
    assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    println!("{:?} {:?} {}", values(), generic::<u8>(), nested());
}
