use std::{mem::MaybeUninit, ptr};

const fn shifted(mut bytes: [u8; 6], right: bool) -> [u8; 6] {
    let p = bytes.as_mut_ptr();
    unsafe {
        if right { ptr::copy(p, p.add(1), 5); }
        else { p.add(1).copy_to(p, 5); }
    }
    bytes
}
const RIGHT: [u8; 6] = shifted(*b"abcdef", true);
const LEFT: [u8; 6] = shifted(*b"abcdef", false);
const WORDS: [u32; 4] = {
    let src = [7u32, 11];
    let mut dst = [0u32; 4];
    unsafe { dst.as_mut_ptr().add(1).copy_from_nonoverlapping(src.as_ptr(), 2); }
    dst
};
const UNINIT: [u8; 3] = {
    let source = [MaybeUninit::new(3u8), MaybeUninit::uninit(), MaybeUninit::new(9)];
    let mut dst = [MaybeUninit::uninit(); 3];
    unsafe {
        ptr::copy_nonoverlapping(source.as_ptr(), dst.as_mut_ptr(), 3);
        dst[1].write(7);
        [dst[0].assume_init(), dst[1].assume_init(), dst[2].assume_init()]
    }
};
const BYTE_WRITES: u32 = {
    let mut value = 0x11223344u32;
    let bytes = [0xaau8, 0xbb];
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), (&raw mut value).cast::<u8>().add(1), 2); }
    value
};
const MAYBE_BYTES: [u8; 2] = {
    let mut dst = [MaybeUninit::<u8>::uninit(); 2];
    unsafe {
        b"ok".as_ptr().copy_to_nonoverlapping(dst.as_mut_ptr().cast(), 2);
        [dst[0].assume_init(), dst[1].assume_init()]
    }
};
const POINTERS: [&u8; 2] = {
    let src = [&17u8, &23u8];
    let mut dst = [&0u8; 2];
    unsafe { ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), 2); }
    dst
};
#[repr(C)]
#[derive(Clone, Copy)]
struct Padded { a: u8, b: u32 }
const PADDED: Padded = {
    let src = Padded { a: 3, b: 77 };
    let mut dst = MaybeUninit::<Padded>::uninit();
    unsafe { (&raw mut *dst.as_mut_ptr()).copy_from(&src, 1); dst.assume_init() }
};
static SOURCE: [u32; 2] = [31, 37];
const STATIC_COPY: [u32; 2] = {
    let mut dst = [0; 2];
    unsafe { SOURCE.as_ptr().copy_to_nonoverlapping(dst.as_mut_ptr(), 2); }
    dst
};
const NONNULL_COPY: [u32; 3] = {
    let mut a = [2u32, 3, 5];
    unsafe {
        let p = ptr::NonNull::new_unchecked(a.as_mut_ptr());
        p.copy_to(p.add(1), 2);
    }
    a
};
const SLICES: [&[u8]; 2] = {
    let src: [&[u8]; 2] = [b"abc", b"de"];
    let mut dst: [&[u8]; 2] = [&[], &[]];
    unsafe { src.as_ptr().copy_to_nonoverlapping(dst.as_mut_ptr(), 2); }
    dst
};
const EMPTY: () = unsafe {
    ptr::copy_nonoverlapping(4usize as *const u32, 8usize as *mut u32, 0);
    ptr::copy(1usize as *const (), 1usize as *mut (), usize::MAX);
};
fn main() {
    assert_eq!(RIGHT, *b"aabcde");
    assert_eq!(LEFT, *b"bcdeff");
    assert_eq!(shifted(*b"abcdef", std::hint::black_box(true)), RIGHT);
    assert_eq!(WORDS, [0, 7, 11, 0]);
    assert_eq!(UNINIT, [3, 7, 9]);
    assert_eq!(BYTE_WRITES.to_ne_bytes(), [0x44, 0xaa, 0xbb, 0x11]);
    assert_eq!(MAYBE_BYTES, *b"ok");
    assert_eq!(*POINTERS[0], 17);
    assert_eq!(*POINTERS[1], 23);
    assert_eq!(PADDED.a, 3);
    assert_eq!(PADDED.b, 77);
    assert_eq!(STATIC_COPY, SOURCE);
    assert_eq!(NONNULL_COPY, [2, 2, 3]);
    assert_eq!(SLICES[0], b"abc");
    assert_eq!(SLICES[1], b"de");
    let _ = EMPTY;
    println!("const copy: overlap, typed storage, pointers and bytes");
}
