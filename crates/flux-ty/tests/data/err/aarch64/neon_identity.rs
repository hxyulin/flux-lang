use core::arch::aarch64::*;
fn main() {
    let v = unsafe { vdupq_n_u8(0) };
    let _: uint16x8_t = v;
    let _: uint64x2_t = v;
    let _ = unsafe { vgetq_lane_u64::<0>(v) };
}
