const WIDTH: usize = 248;

#[inline(never)]
unsafe fn copy_window(dst: *mut [u8; WIDTH], src: *const [u8; WIDTH]) {
    // SAFETY: the caller supplies a readable source and writable destination.
    unsafe { std::ptr::write(dst, *src) };
}

#[inline(never)]
unsafe fn write_after_store(dst: *mut [u8; WIDTH], value: [u8; WIDTH]) {
    // SAFETY: the caller supplies writable storage for one full array.
    unsafe {
        (*dst)[0] = 255;
        std::ptr::write(dst, value);
    }
}

fn main() {
    let mut bytes = [0u8; WIDTH + 9];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let buffer = bytes.as_mut_ptr();
    // SAFETY: both windows fit in the initialized buffer.
    let shifted = unsafe { buffer.add(9) };
    // SAFETY: both pointers refer to complete windows.
    unsafe { copy_window(shifted.cast(), buffer.cast()) };
    for (i, &byte) in bytes[9..].iter().enumerate() {
        assert_eq!(byte, i as u8);
    }
    // SAFETY: both pointers still refer to complete windows.
    unsafe { copy_window(buffer.cast(), shifted.cast()) };
    for (i, &byte) in bytes[..WIDTH].iter().enumerate() {
        assert_eq!(byte, i as u8);
    }
    // SAFETY: the source and destination are inside the initialized buffer.
    unsafe {
        let dst: *mut [u8; WIDTH] = buffer.cast();
        write_after_store(dst, *dst);
    }
    assert_eq!(bytes[0], 0);
    println!("overlap verified");
}
