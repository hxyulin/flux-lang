//! Address arithmetic only: synthetic/null pointers are never dereferenced.
use std::mem::size_of;

#[repr(align(32))]
struct Aligned([u8; 3]);
#[repr(align(64))]
struct AlignedZst;

fn check<T>(address: usize, align: usize, digest: &mut usize) {
    let shared = (address as *const T).align_offset(align);
    let mutable = (address as *mut T).align_offset(align);
    assert_eq!(shared, mutable);
    let stride = size_of::<T>();
    if align <= 128 {
        // Independent bounded oracle; one alignment period suffices.
        let mut expected = usize::MAX;
        for offset in 0..align {
            if address.wrapping_add(stride.wrapping_mul(offset)) & (align - 1) == 0 {
                expected = offset;
                break;
            }
        }
        assert_eq!(shared, expected);
    }
    if shared != usize::MAX {
        assert_eq!(
            address.wrapping_add(stride.wrapping_mul(shared)) & (align - 1),
            0
        );
    }
    *digest = digest.wrapping_mul(31).wrapping_add(shared);
}

fn cases<T>() {
    let mut digest = 0usize;
    for address in 0..257 {
        for bits in 0..8 {
            check::<T>(address, 1usize << bits, &mut digest);
        }
    }
    let high = 1usize << (usize::BITS - 1);
    for bits in 0..usize::BITS {
        let align = 1usize << bits;
        for address in [
            0,
            1,
            2,
            3,
            6,
            17,
            63,
            64,
            65,
            high - 1,
            high,
            high + 1,
            usize::MAX - 7,
            usize::MAX - 2,
            usize::MAX - 1,
            usize::MAX,
            align - 1,
            align,
            align.wrapping_add(1),
        ] {
            check::<T>(address, align, &mut digest);
        }
    }
    println!("stride {} digest {digest}", size_of::<T>());
}

fn main() {
    assert_eq!((4usize as *const u32).align_offset(16), 3);
    assert_eq!((1usize as *const u32).align_offset(16), usize::MAX);
    assert_eq!((1usize as *const [u8; 3]).align_offset(8), 5);
    assert_eq!((usize::MAX as *const u8).align_offset(16), 1);
    assert_eq!((0usize as *const ()).align_offset(64), 0);
    assert_eq!((1usize as *const ()).align_offset(64), usize::MAX);
    cases::<u8>();
    cases::<u16>();
    cases::<u32>();
    cases::<u64>();
    cases::<[u8; 3]>();
    cases::<[u8; 5]>();
    cases::<[u16; 3]>();
    cases::<Aligned>();
    cases::<()>();
    cases::<AlignedZst>();
    // Read this field to keep rustc's dead-code warning out of harness output.
    assert_eq!(Aligned([1, 2, 3]).0[2], 3);
}
