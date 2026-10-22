//! Raw pointers made without references: `&raw const` and `&raw mut`
//! of locals, fields, elements and statics (a `static mut` too, safely),
//! and `core::ptr::{addr_of, addr_of_mut}`.

use core::ptr::{addr_of, addr_of_mut};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

static mut BUF: [u8; 16] = [0; 16];
static mut COUNT: u32 = 0;
static TABLE: [u16; 3] = [10, 20, 30];

struct Pair {
    a: i32,
    b: i64,
}

fn main() {
    let buf = unsafe { &mut *addr_of_mut!(BUF) };
    buf[3] = 9;
    let p = &raw mut COUNT;
    unsafe {
        *p += 5;
        *p += 1;
    }
    show(1, unsafe { *addr_of!(COUNT) } as i64);
    show(2, unsafe { (*addr_of!(BUF))[3] } as i64);
    let t = &raw const TABLE;
    show(3, unsafe { (*t)[2] } as i64);
    let mut pair = Pair { a: 1, b: 2 };
    let pa = &raw mut pair.a;
    let pb = core::ptr::addr_of!(pair.b);
    unsafe {
        *pa = 40;
        show(4, *pa as i64 + *pb);
    }
    let x = 7;
    let px = &raw const x;
    show(5, unsafe { *px } as i64);
    let mut arr = [1, 2, 3];
    let pe = &raw mut arr[1];
    unsafe { *pe = 20 };
    show(6, arr.iter().sum::<i32>() as i64);
    show(7, (&raw const x as usize == px as usize) as i64);
}
