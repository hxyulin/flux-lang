// Integer methods that fail in constants (E0080): those that inherit
// overflow checks overflow where their caller is a constant, and the
// others panic or divide by zero as they do at run time.

const NEXT_POW: u8 = 200u8.next_power_of_two();
const NEXT_MULTIPLE: u8 = 254u8.next_multiple_of(7);
const LOG_ZERO: u32 = 0u32.ilog2();
const LOG10_NEG: u32 = (-5i32).ilog10();
const LOG_BASE: u32 = 100u64.ilog(1);
const SQRT_NEG: i16 = (-4i16).isqrt();
const WRAPPING_DIV: i32 = 1i32.wrapping_div(0);
const SATURATING_DIV: u8 = 1u8.saturating_div(0);
const DIV_EUCLID: i8 = i8::MIN.div_euclid(-1);
const REM_EUCLID: u64 = 7u64.rem_euclid(0);

// These do not fail.
const CHECKED: Option<i32> = i32::MIN.checked_div(-1);
const WRAPPING: i8 = i8::MIN.wrapping_div(-1);
const SATURATING: u8 = 200u8.saturating_add(100);

fn main() {}
