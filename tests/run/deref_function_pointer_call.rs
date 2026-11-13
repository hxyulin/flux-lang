use std::ops::Deref;
use std::cell::Cell;
struct Wrapper<'a, T> { value: T, calls: &'a Cell<u32> }
impl<T> Deref for Wrapper<'_, T> {
    type Target = T;
    fn deref(&self) -> &T { self.calls.set(self.calls.get() + 1); &self.value }
}
fn add(value: u32) -> u32 { value + 3 }
unsafe extern "C" fn twice(value: u32) -> u32 { value * 2 }
fn main() {
    let calls = Cell::new(0);
    let inner = Wrapper { value: add as fn(u32) -> u32, calls: &calls };
    let outer = Wrapper { value: inner, calls: &calls };
    assert_eq!(outer(7), 10);
    assert_eq!(calls.get(), 2);
    assert_eq!((&outer)(8), 11);
    assert_eq!(calls.get(), 4);
    let pointer = Wrapper { value: twice as unsafe extern "C" fn(u32) -> u32, calls: &calls };
    assert_eq!(unsafe { pointer(9) }, 18);
    assert_eq!(calls.get(), 5);
    let boxed = Box::new(add as fn(u32) -> u32);
    assert_eq!(boxed(10), 13);
    println!("{} {}", outer(11), calls.get());
}
