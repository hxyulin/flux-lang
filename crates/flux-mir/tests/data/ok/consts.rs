// Constant evaluation right up to the edges where it would fail.

const MAX_MINUS: i32 = i32::MAX - 1 + 1;
const MIN_PLUS: i32 = i32::MIN + 1 - 1;
const NEG_MAX: i32 = -i32::MAX;
const NEG_LITERAL: i8 = -128;
const MIN_DIV: i32 = i32::MIN / 1;
const MIN_DIV_NEG2: i32 = i32::MIN / -2;
const MIN_REM: i32 = i32::MIN % 1;
const NEG_DIV: i32 = -7 / 2;
const NEG_REM: i32 = -7 % 2;
const SHL_MAX: u32 = 1 << 31;
const SHR_MAX: i64 = i64::MIN >> 63;
const SHL_WIDE: u8 = 1 << 7u64;
const U_MAX: u64 = u64::MAX / 3 * 3;
const U_SUB: u8 = 255 - u8::MAX;
const BIG: u128 = u128::MAX - u64::MAX as u128;
const MUL_EDGE: i16 = 181 * 181;
const MUL_NEG: i16 = -128 * 256;
const CAST_WRAP: u8 = (u8::MAX as i32 + 1) as u8;
const CAST_SIGN: i8 = 200u8 as i8;
const CHAIN: i32 = MAX_MINUS + MIN_PLUS.wrapping_add(i32::MAX);
const WRAPS: u8 = u8::MAX.wrapping_add(1).wrapping_sub(1).wrapping_mul(3);
const NEG_WRAP: i32 = i32::MIN.wrapping_neg();
const ABS: i32 = (i32::MIN + 1).abs();
const POW: i32 = 2i32.pow(30) + (-2i32).pow(31);
const EUCLID: i32 = i32::MIN.rem_euclid(2) + (-7i32).rem_euclid(3);
const UNREACHED: i32 = if MAX_MINUS > 0 { 1 } else { i32::MAX + 1 };
const SHORT: bool = false && 1 / ZERO == 0;
const ZERO: i32 = 0;
const LOOP: u64 = factorial(20);
const NESTED: i32 = add_checked(i32::MAX - 10, 10);

struct Limits;

impl Limits {
    const HALF: i32 = i32::MAX / 2;
    const NEXT: i32 = Self::HALF + 1;
}

const fn factorial(n: u64) -> u64 {
    let mut acc = 1;
    let mut i = 2;
    while i <= n {
        acc *= i;
        i += 1;
    }
    acc
}

const fn add_checked(a: i32, b: i32) -> i32 {
    if b > 0 && a > i32::MAX - b { 0 } else { a + b }
}

fn main() {
    let a = MAX_MINUS as i64 + MIN_PLUS as i64 + NEG_MAX as i64 + NEG_LITERAL as i64 + MIN_DIV as i64;
    let b = MIN_DIV_NEG2 as i64 + MIN_REM as i64 + NEG_DIV as i64 + NEG_REM as i64 + CHAIN as i64;
    let b = b + NEG_WRAP as i64 + ABS as i64 + POW as i64 + EUCLID as i64;
    let c = SHL_MAX as u64 + (SHR_MAX == -1) as u64 + SHL_WIDE as u64 + U_MAX / 4 + U_SUB as u64 + (BIG >> 120) as u64;
    let d = MUL_EDGE as i32 + MUL_NEG as i32 + CAST_WRAP as i32 + CAST_SIGN as i32 + WRAPS as i32 + UNREACHED;
    let e = SHORT as u64 + LOOP + NESTED as u64 + Limits::NEXT as u64 + Limits::HALF as u64;
    let _all = (a, b, c, d, e);
}
