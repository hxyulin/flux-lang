//! Transmutes between integers and floats of one size, and the
//! `to_bits`/`from_bits` methods built on them, keep every bit: NaN
//! payloads, signed zeros and infinities.

#![allow(unnecessary_transmutes)]

use core::hint::black_box;
use std::mem::transmute;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn hex(n: u64) {
    unsafe {
        printf(c"%llx\n".as_ptr(), n);
    }
}

fn show(x: f64) {
    unsafe {
        printf(c"%g\n".as_ptr(), x);
    }
}

#[inline(never)]
fn round_trip_32(bits: u32) -> u32 {
    let x: f32 = unsafe { transmute(bits) };
    let y = black_box(x);
    unsafe { transmute::<f32, u32>(y) }
}

#[inline(never)]
fn round_trip_64(bits: u64) -> u64 {
    let x = f64::from_bits(bits);
    black_box(x).to_bits()
}

fn main() {
    let words32 = [
        0x7fc0_0000u32, // quiet NaN
        0x7f80_0001,    // signaling NaN, payload 1
        0xffc1_2345,    // negative NaN with a payload
        0x8000_0000,    // -0.0
        0x7f80_0000,    // infinity
        0xff80_0000,    // negative infinity
        0x0000_0001,    // the smallest subnormal
        0x3f80_0000,    // 1.0
    ];
    for bits in words32 {
        hex(round_trip_32(black_box(bits)) as u64);
        let x = f32::from_bits(black_box(bits));
        let flags = (x.is_nan() as u64) << 12
            | (x.is_infinite() as u64) << 8
            | (x.is_sign_negative() as u64) << 4
            | x.is_subnormal() as u64;
        hex(flags);
        let signed: i32 = unsafe { transmute(x) };
        hex(signed as u32 as u64);
    }
    let words64 = [
        0x7ff8_0000_0000_0000u64,
        0x7ff0_0000_0000_0001,
        0xfff8_dead_beef_0001,
        0x8000_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x3ff0_0000_0000_0000,
    ];
    for bits in words64 {
        hex(round_trip_64(black_box(bits)));
        let x: f64 = unsafe { transmute(black_box(bits as i64)) };
        let flags = (x.is_nan() as u64) << 12
            | (x.is_infinite() as u64) << 8
            | (x.is_sign_negative() as u64) << 4
            | x.is_subnormal() as u64;
        hex(flags);
        hex(x.to_bits());
    }
    // Values from arithmetic, then back.
    let zero = black_box(0.0f64);
    let negative_zero = -zero;
    hex(negative_zero.to_bits());
    show(f64::from_bits(negative_zero.to_bits()));
    hex((1.0f32 / black_box(0.0f32)).to_bits() as u64);
    hex((-1.0f64 / zero).to_bits());
    show(f64::from_bits(black_box(0x4009_21fb_5444_2d18)));
    show(f32::from_bits(black_box(0x4049_0fdb)) as f64);
    show(black_box(-2.5f64).abs());
    show(black_box(-0.0f32).abs() as f64);
    hex(black_box(-0.0f32).abs().to_bits() as u64);
    // Constants, which the optimizer may fold.
    hex(f32::NAN.to_bits() as u64);
    hex(f64::NEG_INFINITY.to_bits());
    hex(unsafe { transmute::<f64, u64>(-0.0) });
    hex(unsafe { transmute::<u32, f32>(0x7f80_0002) }.to_bits() as u64);
}
