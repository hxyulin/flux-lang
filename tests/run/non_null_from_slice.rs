use std::mem::{MaybeUninit, align_of, size_of};
use std::ptr::NonNull;

#[repr(align(32))]
struct Aligned(u8);

fn shared<T: ?Sized>(value: &T) -> NonNull<T> {
    NonNull::from(value)
}

fn mutable<T: ?Sized>(value: &mut T) -> NonNull<T> {
    NonNull::from(value)
}

const fn length<T>(pointer: NonNull<[T]>) -> usize {
    pointer.len()
}

fn main() {
    let value = Aligned(7);
    let pointer = shared(&value);
    assert_eq!(pointer.as_ptr() as usize, &value as *const Aligned as usize);
    assert_eq!(pointer.as_ptr() as usize % align_of::<Aligned>(), 0);
    assert_eq!(value.0, 7);
    assert_eq!(size_of::<Option<NonNull<u8>>>(), size_of::<*mut u8>());
    assert_eq!(size_of::<Option<NonNull<[u8]>>>(), size_of::<*mut [u8]>());

    let mut values = [11u32, 22, 33, 44];
    let address = values.as_mut_ptr() as usize;
    let pointer = mutable(&mut values[..]);
    assert_eq!(pointer.as_ptr() as *mut u32 as usize, address);
    assert_eq!(length(pointer), 4);
    assert_eq!(length(pointer.clone()), 4);
    // SAFETY: The pointer came from the live, exclusively borrowed array.
    unsafe {
        (*pointer.as_ptr())[1] = 55;
    }
    assert_eq!(values[1], 55);
    let middle = shared(&values[1..3]);
    assert_eq!(
        middle.as_ptr() as *mut u32 as usize,
        address + size_of::<u32>()
    );
    assert_eq!(middle.len(), 2);
    let empty = shared(&values[2..2]);
    assert_eq!(empty.len(), 0);
    assert_eq!(
        empty.as_ptr() as *mut u32 as usize,
        address + 2 * size_of::<u32>()
    );
    let units = [(); 19];
    assert_eq!(shared(&units[..]).len(), 19);
    assert_eq!(shared(&units[..0]).len(), 0);
    let mut units = [(); 23];
    assert_eq!(mutable(&mut units[..]).len(), 23);

    // Metadata queries must not read uninitialized chunk contents.
    let chunk = Box::<[Aligned]>::new_uninit_slice(17);
    let chunk = NonNull::from(Box::leak(chunk));
    assert_eq!(chunk.len(), 17);
    assert_eq!(chunk.cast::<u8>().as_ptr() as usize % 32, 0);
    // SAFETY: This reconstructs the original allocation exactly once; its
    // MaybeUninit elements do not require initialized Aligned values.
    unsafe {
        drop(Box::from_raw(chunk.as_ptr()));
    }
    let _ = size_of::<MaybeUninit<Aligned>>();
    println!("NonNull conversions: identity, alignment, slices, empty, ZST, chunks");
}
