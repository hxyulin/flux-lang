//! `as` casts from i8, u8, i16 and u16 to f32 and f64, at the types'
//! bounds and from runtime values.

use core::hint::black_box;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(x: f64) {
    unsafe {
        printf(c"%.1f\n".as_ptr(), x);
    }
}

fn i8s(n: i8) {
    show(n as f32 as f64);
    show(n as f64);
}

fn u8s(n: u8) {
    show(n as f32 as f64);
    show(n as f64);
}

fn i16s(n: i16) {
    show(n as f32 as f64);
    show(n as f64);
}

fn u16s(n: u16) {
    show(n as f32 as f64);
    show(n as f64);
}

fn main() {
    for n in [i8::MIN, -1, 0, 1, i8::MAX] {
        i8s(black_box(n));
    }
    for n in [u8::MIN, 1, 128, 200, u8::MAX] {
        u8s(black_box(n));
    }
    for n in [i16::MIN, -129, -1, 0, 255, i16::MAX] {
        i16s(black_box(n));
    }
    for n in [u16::MIN, 1, 32768, 40000, u16::MAX] {
        u16s(black_box(n));
    }
    // Constants, which the optimizer may fold.
    show(i8::MIN as f32 as f64);
    show(u8::MAX as f32 as f64);
    show(i16::MIN as f64);
    show(u16::MAX as f64);
    // A narrow result of arithmetic, whose register may hold bits beyond
    // its width.
    let a: u8 = black_box(250);
    let b: u8 = black_box(10);
    show(a.wrapping_add(b) as f64);
    let c: i8 = black_box(127);
    show(c.wrapping_add(1) as f32 as f64);
    let d: u16 = black_box(65535);
    show(d.wrapping_mul(3) as f64);
    let e: i16 = black_box(-32768);
    show(e.wrapping_sub(1) as f32 as f64);
}
