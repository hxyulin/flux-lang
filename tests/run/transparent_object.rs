use std::cell::Cell;
use std::mem::{ManuallyDrop, size_of, size_of_val, align_of, align_of_val};
use std::ops::Deref;
#[repr(transparent)]
struct Wrapped<T: ?Sized>(ManuallyDrop<T>);
impl<T: ?Sized> Deref for Wrapped<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}
trait Value { fn value(&self) -> u32; }
struct Count<'a> { drops: &'a Cell<u32>, value: u32 }
impl Value for Count<'_> { fn value(&self) -> u32 { self.value } }
impl Drop for Count<'_> { fn drop(&mut self) { self.drops.set(self.drops.get() + 1); } }
fn main() {
    let drops = Cell::new(0u32);
    let object: Box<Wrapped<dyn Value + '_>> = Box::new(Wrapped(ManuallyDrop::new(Count { drops: &drops, value: 42 })));
    assert_eq!(object.value(), 42);
    assert_eq!(size_of_val(&*object), size_of::<Count<'_>>());
    assert_eq!(align_of_val(&*object), align_of::<Count<'_>>());
    drop(object);
    assert_eq!(drops.get(), 0);
    println!("transparent object 42");
}
