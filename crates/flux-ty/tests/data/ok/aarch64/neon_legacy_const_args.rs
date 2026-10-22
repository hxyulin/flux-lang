use core::arch::aarch64::*;
use core::arch::aarch64::vgetq_lane_u64 as lane;
const ZERO: i32 = 0;
macro_rules! extract { ($a:expr, $n:expr) => { lane($a, $n) }; }
fn extract<const N: i32>(a: uint64x2_t) -> u64 {
    unsafe { lane(a, N) }
}
fn narrow(a: uint16x8_t) -> uint8x8_t {
    const SHIFT: i32 = 4;
    unsafe { vshrn_n_u16(a, SHIFT) }
}
fn lanes(a: uint64x2_t) -> u64 {
    unsafe { extract!(a, ZERO) + lane(a, -0) + lane(a, (1)) + lane::<1>(a) }
}
fn main() {}
