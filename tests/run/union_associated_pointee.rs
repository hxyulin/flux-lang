use core::mem::{ManuallyDrop, MaybeUninit};
use core::ptr::NonNull;

trait Array {
    type Item;
}

impl<T, const N: usize> Array for [T; N] {
    type Item = T;
}

union Storage<A: Array> {
    inline: ManuallyDrop<MaybeUninit<A>>,
    heap: (NonNull<A::Item>, usize),
}

impl<A: Array> Storage<A> {
    fn heap(ptr: NonNull<A::Item>, capacity: usize) -> Self {
        Self { heap: (ptr, capacity) }
    }
}

fn main() {
    let value = 42u32;
    let ptr = NonNull::from(&value);
    let storage = Storage::<[u32; 8]>::heap(ptr, 8);
    // SAFETY: The heap field was initialized and `value` is still live.
    let (stored, capacity) = unsafe { storage.heap };
    assert_eq!(stored, ptr);
    assert_eq!(capacity, 8);
    println!("{capacity}");
}
