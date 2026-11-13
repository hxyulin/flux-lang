use std::pin::Pin;
use std::ptr::NonNull;
use std::marker::PhantomPinned;
trait Value { fn value(&self) -> u32; }
struct Number { value: u32, _pin: PhantomPinned }
impl Value for Number { fn value(&self) -> u32 { self.value } }
fn pin<F: Value + 'static>(value: Box<F>) -> Pin<Box<dyn Value>> { Pin::from(value) }
fn main() {
    let pinned = pin(Box::new(Number { value: 37, _pin: PhantomPinned }));
    assert_eq!(pinned.as_ref().get_ref().value(), 37);
    let direct: Pin<Box<dyn Value>> = Pin::from(Box::new(Number { value: 41, _pin: PhantomPinned }));
    assert_eq!(direct.as_ref().get_ref().value(), 41);
    let mut number = Number { value: 53, _pin: PhantomPinned };
    let pointer: NonNull<dyn Value> = NonNull::from(&mut number);
    // SAFETY: number remains live and is not accessed through conflicting references.
    unsafe { assert_eq!(pointer.as_ref().value(), 53); }
    println!("contextual pin conversion complete");
}
