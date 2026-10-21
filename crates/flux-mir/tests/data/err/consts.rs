// Constants whose evaluation fails (E0080).

const ADD: i32 = i32::MAX + 1;
const SUB: u8 = 0 - 1;
const MUL: i64 = i64::MAX * 2;
const NEG: i32 = -i32::MIN;
const NEG_I8: i8 = -(-128i8);
const SHL: u32 = 1 << 32;
const SHR: i64 = 1 >> 64;
const SHL_NEG: i32 = 1 << -1;
const DIV_ZERO: i32 = 1 / 0;
const REM_ZERO: u64 = 7 % 0;
const DIV_MIN: i32 = i32::MIN / -1;
const REM_MIN: i8 = -128i8 % -1;
const U64: u64 = u64::MAX + 1;
const NESTED: i32 = (i32::MAX - 1) + (2 * 1);
const UNUSED: u16 = u16::MAX * u16::MAX;
const _: i32 = i32::MAX + 1;

// Through another constant: the error is reported where it happens.
const BASE: i32 = i32::MAX;
const FROM_BASE: i32 = BASE + 1;
const ZERO: i32 = 0;
const FROM_ZERO: i32 = 10 / ZERO;
const BAD: i32 = i32::MIN - 1;
const USES_BAD: i32 = BAD;

// Through a `const fn` call: the error points at the call.
const fn inc(x: i32) -> i32 {
    x + 1
}

const fn div(a: i32, b: i32) -> i32 {
    a / b
}

const fn deep(x: i32) -> i32 {
    inc(x) * 2
}

const CALL_INC: i32 = inc(i32::MAX);
const CALL_DIV: i32 = 1 + div(1, ZERO);
const CALL_DEEP: i32 = deep(i32::MAX / 2 + 1);
const LOOP: u8 = {
    let mut x: u8 = 1;
    let mut i = 0;
    while i < 8 {
        x *= 2;
        i += 1;
    }
    x
};

// `core`'s methods panic on overflow during constant evaluation in rustc.
const ABS: i8 = i8::MIN.abs();
const POW: i32 = 2i32.pow(31);
const EUCLID: i32 = 7i32.rem_euclid(0);

struct S;

impl S {
    const USED: i32 = i32::MAX + 1;
    const VIA_SELF: i32 = Self::HALF * 3;
    const HALF: i32 = i32::MAX / 2;
}

fn main() {
    let _a = ADD + NEG + S::USED + S::VIA_SELF + USES_BAD + FROM_BASE + FROM_ZERO;
    let _s = S;
}
