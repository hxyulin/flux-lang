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
    println!("volatile {} {} {}", word, wrapped.0, letter);
}
