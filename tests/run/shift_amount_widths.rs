//! Shifts whose amount has a different integer type than the shifted
//! value, on u8, i32, i64 and i128, with constant and runtime amounts.

use core::hint::black_box;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: u64) {
    unsafe {
        printf(c"%llx\n".as_ptr(), n);
    }
}

fn wide(n: u128) {
    show((n >> 64) as u64);
    show(n as u64);
}

fn main() {
    // u8 values.
    let x: u8 = black_box(0b1011_0101);
    let n64: u64 = black_box(3);
    let n128: u128 = black_box(7);
    let n16: i16 = black_box(5);
    show((x << n64) as u64);
    show((x >> n64) as u64);
    show((x << n128) as u64);
    show((x >> n16) as u64);
    show((x << 3u64) as u64);
    show((x >> 7u128) as u64);
    show(x.wrapping_shl(black_box(11)) as u64);
    show(x.wrapping_shr(black_box(13)) as u64);
    show(x.wrapping_shl(9) as u64);
    show(x.wrapping_shr(255) as u64);
    show(x.wrapping_shl(black_box(u32::MAX)) as u64);

    // i32 values, signed shifts right.
    let y: i32 = black_box(-0x1234_5679);
    let m8: u8 = black_box(31);
    let m64: i64 = black_box(4);
    let m128: u128 = black_box(17);
    show((y << m8) as u32 as u64);
    show((y >> m8) as u32 as u64);
    show((y >> m64) as u32 as u64);
    show((y << m128) as u32 as u64);
    show((y >> 12u8) as u32 as u64);
    show((y << 20i64) as u32 as u64);
    show(y.wrapping_shl(black_box(35)) as u32 as u64);
    show(y.wrapping_shr(black_box(63)) as u32 as u64);
    show(y.wrapping_shr(40) as u32 as u64);
    show((y as u32).wrapping_shr(black_box(36)) as u64);

    // i64 values.
    let z: i64 = black_box(-0x0123_4567_89ab_cdef);
    let k8: i8 = black_box(60);
    let k16: u16 = black_box(33);
    let k128: i128 = black_box(1);
    show((z << k8) as u64);
    show((z >> k8) as u64);
    show(((z as u64) >> k16) as u64);
    show((z << k128) as u64);
    show((z >> 40u8) as u64);
    show(z.wrapping_shl(black_box(70)) as u64);
    show(z.wrapping_shr(black_box(127)) as u64);
    show((z as u64).wrapping_shr(65) as u64);

    // i128 values, with u32, u8 and u64 amounts.
    let w: i128 = black_box(-0x0123_4567_89ab_cdef_0011_2233_4455_6677);
    let j32: u32 = black_box(68);
    let j8: u8 = black_box(127);
    let j64: u64 = black_box(3);
    wide((w << j32) as u128);
    wide((w >> j32) as u128);
    wide(((w as u128) >> j32) as u128);
    wide((w >> j8) as u128);
    wide(((w as u128) >> j8) as u128);
    wide((w << j64) as u128);
    wide((w << 100u32) as u128);
    wide((w >> 64u8) as u128);
    wide(((w as u128) >> 1u16) as u128);
    wide(w.wrapping_shl(black_box(200)) as u128);
    wide(w.wrapping_shr(black_box(129)) as u128);
    wide((w as u128).wrapping_shr(black_box(255)));
    wide(w.wrapping_shl(64) as u128);
    wide((w as u128).wrapping_shr(130));
}
