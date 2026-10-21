// Arithmetic rustc's `arithmetic_overflow` and `unconditional_panic` lints
// accept: values it cannot know, and wrapping. (Overflow in a branch rustc
// knows is not taken is in `err/lints_unreachable.rs`: flux reports it.)

fn unknown(c: bool, n: u8, d: i32) -> i32 {
    // A local assigned in another block has no known value afterwards.
    let mut z: u8 = 250;
    if c {
        z = 1;
    }
    let a = z + 10;
    // Parameters are unknown.
    let b = n + 255;
    let q = 100 / d;
    // A local assigned in a loop is unknown.
    let mut i: u8 = 0;
    while i < 5 {
        i += 1;
    }
    let e = i * 30;
    // A borrowed local is unknown.
    let mut k: u8 = 255;
    let r = &mut k;
    *r = 0;
    let f = k + 1;
    a as i32 + b as i32 + q + e as i32 + f as i32
}

fn wrapping() -> u64 {
    let a = u8::MAX.wrapping_add(1);
    let b = 0u16.wrapping_sub(1);
    let c = i32::MIN.wrapping_neg();
    let d = i64::MAX.wrapping_mul(2);
    let e = 1u32.rotate_left(33);
    a as u64 + b as u64 + c as u32 as u64 + d as u32 as u64 + e as u64
}

fn edges() -> i64 {
    let a = i32::MAX - 1 + 1;
    let b = i32::MIN / 1;
    let c = -128i8;
    let d = 1u64 << 63;
    let e = i8::MIN % 1;
    let m = 255u8;
    let f = m - 255;
    a as i64 + b as i64 + c as i64 + (d >> 63) as i64 + e as i64 + f as i64
}

fn main() {
    let _ = unknown(true, 0, 1) as i64 + wrapping() as i64 + edges();
}
