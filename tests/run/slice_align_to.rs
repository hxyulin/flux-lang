#[repr(align(64))]
struct Storage([u8; 512]);

#[repr(C)]
struct Twelve([u32; 3]);

#[repr(C, align(8))]
struct TwentyFour([u8; 24]);

#[repr(C, align(8))]
struct Sixteen([u8; 16]);

#[repr(align(64))]
struct Zero;

fn check<T, U>(input: &[T]) -> (usize, usize, usize) {
    // SAFETY: this driver only instantiates integer and byte-array types with
    // fully initialized storage and no invalid bit patterns or references.
    let (prefix, middle, suffix) = unsafe { input.align_to::<U>() };
    let t_size = std::mem::size_of::<T>();
    let u_size = std::mem::size_of::<U>();
    assert_eq!(prefix.as_ptr(), input.as_ptr());
    if t_size != 0 && u_size != 0 {
        assert_eq!(
            prefix.len() * t_size + middle.len() * u_size + suffix.len() * t_size,
            input.len() * t_size
        );
        assert_eq!(middle.as_ptr() as usize % std::mem::align_of::<U>(), 0);
        if !middle.is_empty() {
            assert_eq!(
                middle.as_ptr() as usize,
                input.as_ptr() as usize + prefix.len() * t_size
            );
        }
        if !suffix.is_empty() {
            assert_eq!(
                suffix.as_ptr() as usize,
                input.as_ptr() as usize + prefix.len() * t_size + middle.len() * u_size
            );
        }
    } else {
        assert_eq!(prefix.len(), input.len());
        assert!(middle.is_empty() && suffix.is_empty());
    }
    (prefix.len(), middle.len(), suffix.len())
}

fn main() {
    let mut storage = Storage([0u8; 512]);
    for (index, byte) in storage.0.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let mut total = 0;
    for offset in 0..16 {
        for length in 0..65 {
            let input = &storage.0[offset..offset + length];
            let (prefix, middle, suffix) = check::<u8, u64>(input);
            let expected = (8 - offset % 8) % 8;
            if expected > length {
                assert_eq!((prefix, middle, suffix), (length, 0, 0));
            } else {
                assert_eq!(
                    (prefix, middle, suffix),
                    (expected, (length - expected) / 8, (length - expected) % 8)
                );
            }
            total += prefix + middle + suffix;
        }
    }
    // SAFETY: these types have only initialized byte/integer fields, no
    // padding, and alignment satisfied by Storage. All views stay in bounds.
    unsafe {
        let twelves = std::slice::from_raw_parts(storage.0.as_ptr() as *const Twelve, 32);
        let sixteen = std::slice::from_raw_parts(storage.0.as_ptr() as *const Sixteen, 24);
        for offset in 0..4 {
            for length in 0..20 {
                println!(
                    "pairs {:?} {:?}",
                    check::<Twelve, u64>(&twelves[offset..offset + length]),
                    check::<Sixteen, TwentyFour>(&sixteen[offset..offset + length])
                );
            }
        }
        let odd = std::slice::from_raw_parts(storage.0.as_ptr().add(1) as *const [u8; 2], 10);
        assert_eq!(check::<[u8; 2], u16>(odd), (10, 0, 0));
    }
    assert_eq!(check::<(), u64>(&[(); 7]), (7, 0, 0));
    assert_eq!(check::<u8, ()>(&storage.0[..7]), (7, 0, 0));
    assert_eq!(check::<u8, Zero>(&storage.0[..7]), (7, 0, 0));
    println!("align-to {}", total);
}
