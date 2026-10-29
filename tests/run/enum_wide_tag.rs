//! An enum whose tag is widened to its first field's alignment: every
//! byte of the 16-byte tag of `E` is written, including where the
//! discriminant is stored from a runtime value.

#![allow(dead_code)]

use core::hint::black_box;
use std::mem::{size_of, transmute};

#[derive(Clone, Copy)]
enum E {
    A(u128),
    B,
}

#[inline(never)]
fn make(b: bool) -> E {
    if black_box(b) { E::B } else { E::A(7) }
}

fn show(words: [u64; 4]) {
    println!("{:x} {:x}", words[0], words[1]);
}

fn main() {
    println!("{}", size_of::<E>());
    show(unsafe { transmute(E::B) });
    show(unsafe { transmute(make(true)) });
    let words: [u64; 4] = unsafe { transmute(make(false)) };
    println!("{:x} {:x} {:x} {:x}", words[0], words[1], words[2], words[3]);
    // Over memory holding ones, so a byte left unwritten shows.
    let mut buffer = [u64::MAX; 4];
    for b in [true, false] {
        unsafe { core::ptr::write(&mut buffer as *mut [u64; 4] as *mut E, make(b)) };
        show(black_box(buffer));
    }
    let mut buffer = [u64::MAX; 4];
    unsafe { core::ptr::write(&mut buffer as *mut [u64; 4] as *mut E, black_box(E::B)) };
    show(black_box(buffer));
}
