use std::ptr::{read_volatile, write_volatile};
#[derive(Copy, Clone)]
struct Word(u32);
#[inline(never)]
fn accesses(pointer: *mut u32) -> u32 {
    unsafe {
        write_volatile(pointer, 1);
        let _ = read_volatile(pointer);
        write_volatile(pointer, 2);
        read_volatile(pointer)
    }
}
fn main() {
    #[derive(Copy,Clone)]
    struct Pair(u64,u64);
    let mut pair=std::mem::MaybeUninit::new(Pair(3,5));
    // SAFETY: both pointers refer to initialized, aligned local storage.
    unsafe {
        write_volatile(&mut pair,std::mem::MaybeUninit::new(Pair(11,17)));
        let value=read_volatile(&pair).assume_init();
        assert_eq!((value.0,value.1),(11,17));
        let mut wide=&[2u8,3][..] as *const [u8];
        let bytes=[7u8,11,13];
        write_volatile(&mut wide,&bytes[..] as *const [u8]);
        assert_eq!(&*read_volatile(&wide),&bytes);
        let mut large=3u128;
        write_volatile(&mut large,0x112233445566778899aabbccddeeff00);
        assert_eq!(read_volatile(&large),0x112233445566778899aabbccddeeff00);
        let mut array=[1u32,2];
        write_volatile(&mut array,[7,19]);
        assert_eq!(read_volatile(&array),[7,19]);
        let mut optional=Some(1u8);
        write_volatile(&mut optional,None);
        assert_eq!(read_volatile(&optional),None);
    }

    let mut word = 17u32;
    assert_eq!(accesses(&mut word), 2);
    assert_eq!(word, 2);
    let mut wrapped = Word(5);
    unsafe {
        write_volatile(&mut wrapped, Word(73));
        assert_eq!(read_volatile(&wrapped).0, 73);
    }
    let mut byte = std::mem::MaybeUninit::new(9u8);
    unsafe {
        write_volatile(&mut byte, std::mem::MaybeUninit::new(44));
        assert_eq!(read_volatile(&byte).assume_init(), 44);
    }
    let mut boolean = false;
    let mut letter = 'a';
    let mut double = -0.0f64;
    unsafe {
        (&mut boolean as *mut bool).write_volatile(true);
        assert!(read_volatile(&boolean));
        write_volatile(&mut letter, '中');
        assert_eq!(read_volatile(&letter), '中');
        write_volatile(&mut double, f64::from_bits(0x7ff8_1234_5678_90ab));
        assert_eq!(read_volatile(&double).to_bits(), 0x7ff8_1234_5678_90ab);
    }
    let value = 19;
    let mut pointer = &value as *const i32;
    unsafe {
        write_volatile(&mut pointer, &word as *const u32 as *const i32);
        assert_eq!(
            read_volatile(&pointer) as usize,
            &word as *const u32 as usize
        );
    }
    let mut zero = ();
    unsafe {
        write_volatile(&mut zero, ());
        read_volatile(&zero);
    }
    #[repr(C)]struct Frame{before:u64,words:[u64;16],after:u64}
    let mut frame=Frame{before:13,words:[0;16],after:17};
    // SAFETY: the complete array is initialized, aligned and exclusively borrowed.
    unsafe{write_volatile(&raw mut frame.words,[37;16]);assert_eq!(read_volatile(&raw const frame.words),[37;16]);}
    assert_eq!(frame.before,13);assert_eq!(frame.after,17);
    let mut tail=[0u8;131];
    // SAFETY: the byte array is initialized and the full access remains in bounds.
    unsafe{write_volatile(&raw mut tail,[23;131]);assert_eq!(read_volatile(&raw const tail),[23;131]);}
    #[repr(align(64))]struct LargeAligned(u32);
    let mut aligned_value=LargeAligned(7);
    // SAFETY: both accesses cover the complete, aligned initialized aggregate.
    unsafe{write_volatile(&raw mut aligned_value,LargeAligned(9));assert_eq!(read_volatile(&raw const aligned_value).0,9);}
    println!("volatile {} {} {}", word, wrapped.0, letter);
}
