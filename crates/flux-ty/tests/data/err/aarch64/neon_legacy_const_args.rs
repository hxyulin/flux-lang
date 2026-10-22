use core::arch::aarch64::*;
fn invoke<F: Fn(uint64x2_t) -> u64>(f: F) {}
fn bad(a: uint64x2_t) {
    let _ = unsafe { vgetq_lane_u64::<0>(a, 0) };
    let _ = unsafe { (vgetq_lane_u64::<0>)(a, 0) };
    let item = vgetq_lane_u64::<0>;
    let _ = unsafe { item(a, 0) };
    let pointer: unsafe fn(uint64x2_t) -> u64 = vgetq_lane_u64::<0>;
    let _ = unsafe { pointer(a, 0) };
    let _ = vgetq_lane_u64(a, 0);
    let _: fn(uint64x2_t) -> u64 = vgetq_lane_u64::<0>;
    invoke(vgetq_lane_u64::<0>);
}
fn main() {}
