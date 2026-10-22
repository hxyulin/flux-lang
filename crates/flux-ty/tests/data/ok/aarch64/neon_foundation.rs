use core::arch::aarch64::*;
fn copy_debug<T: Copy + core::fmt::Debug>(x: T) -> T {
    x
}
unsafe fn vector(p: *const u8) -> uint64x2_t {
    unsafe {
        let a = copy_debug(vld1q_u8(p));
        let b = vdupq_n_u8(128);
        let c = vorrq_u8(vandq_u8(a, b), vceqq_u8(a, b));
        let _ = vreinterpretq_u16_u8(c);
        vreinterpretq_u64_u8(c)
    }
}
fn register_ops(a: uint8x16_t, b: uint8x16_t) -> u64 {
    unsafe {
    let c = vorrq_u8(vandq_u8(a, b), vceqq_u8(a, vdupq_n_u8(255)));
    let _ = vreinterpretq_u16_u8(c);
    vgetq_lane_u64::<1>(vreinterpretq_u64_u8(c))
    }
}
fn main() {}
