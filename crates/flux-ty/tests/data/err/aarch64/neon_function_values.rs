use core::arch::aarch64::*;
fn accepts_fn<F: Fn(u8) -> uint8x16_t>(f: F) { let _ = f(1); }
fn main() {
    let _splat: fn(u8) -> uint8x16_t = vdupq_n_u8;
    let _lane: fn(uint64x2_t) -> u64 = vgetq_lane_u64::<1>;
    let _words: fn(uint8x16_t) -> uint64x2_t = vreinterpretq_u64_u8;
    let _cast = vdupq_n_u8 as fn(u8) -> uint8x16_t;
    accepts_fn(vdupq_n_u8);
    let direct = vdupq_n_u8;
    let _ = direct(1);
    let _ = vdupq_n_u8(2);
}
