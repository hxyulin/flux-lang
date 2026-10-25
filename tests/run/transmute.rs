use std::mem::transmute;

const BYTES: [u8; 8] = unsafe { transmute(0x0123456789abcdefu64) };
const WORD: u64 = unsafe { transmute([1u8, 2, 3, 4, 5, 6, 7, 8]) };
const FLAG: bool = unsafe { transmute(1u8) };
const LETTER: char = unsafe { transmute(0x1f980u32) };
const ADDRESS: *const u32 = unsafe { transmute(4096usize) };
#[repr(C)]
struct Words { first: u16, second: u16, third: u16, fourth: u16 }
#[repr(transparent)]
struct Owner(Box<i32>);
fn identity<T>(x: T) -> T { unsafe { transmute(x) } }
fn invalid_mut<T>(x: usize) -> *mut T { unsafe { transmute(x) } }
fn plus(x: u32) -> u32 { x + 4 }
const CALL: unsafe fn(u32) -> u32 = unsafe { transmute(plus as fn(u32)->u32) };
fn convert(x: u64) {
    let bytes: [u8; 8] = unsafe { transmute(x) };
    let word: u64 = unsafe { transmute(bytes) };
    let words: Words = unsafe { transmute(x) };
    let again: u64 = unsafe { transmute(words) };
    println!("{} {} {} {}", word, again, bytes[0], bytes[7]);
}
fn main() {
    println!("{} {} {} {} {}", WORD, BYTES[0], BYTES[7], FLAG, LETTER as u32);
    assert_eq!(ADDRESS as usize, 4096);
    for x in [0, 1, 0x0123456789abcdef, u64::MAX] { convert(x); }
    let mut n = 42u32;
    let address = &mut n as *mut u32 as usize;
    let pointer = invalid_mut::<u32>(address);
    unsafe { *pointer += 3; }
    println!("{} {} {}", n, identity(81u32), unsafe { CALL(6) });
    let owner: Owner = unsafe { transmute(Box::new(19)) };
    println!("{}", *owner.0);
}
