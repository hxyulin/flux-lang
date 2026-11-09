use std::ptr::NonNull;

fn slice(pointer: NonNull<[u32; 3]>) -> NonNull<[u32]> { pointer }

fn main() {
    let mut values = [11, 22, 33];
    let mut pointer = slice(NonNull::from(&mut values));
    assert_eq!(pointer.len(), 3);
    // SAFETY: values remains live, initialized and exclusively accessed here.
    unsafe { pointer.as_mut()[1] = 42; }
    assert_eq!(values, [11, 42, 33]);
    let empty: NonNull<[u32]> = NonNull::<[u32; 0]>::dangling();
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.as_ptr() as *mut u32 as usize % std::mem::align_of::<u32>(), 0);
    println!("{} {} {}", pointer.len(), values[1], empty.len());
}
