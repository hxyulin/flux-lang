use core::arch::aarch64::*;

fn apply<F: Fn(u8) -> uint8x16_t>(f: F, byte: u8) -> uint8x16_t {
    f(byte)
}

fn main() {
    let direct = vdupq_n_u8;
    unsafe { println!("{:?}", direct(3)); }
    // Target-feature items can be reified as unsafe pointers, while a closure
    // calling them inside an unsafe block still implements the Fn traits.
    let splat: unsafe fn(u8) -> uint8x16_t = vdupq_n_u8;
    let load: unsafe fn(*const u8) -> uint8x16_t = vld1q_u8;
    let words: unsafe fn(uint8x16_t) -> uint64x2_t = vreinterpretq_u64_u8;
    let lane: unsafe fn(uint64x2_t) -> u64 = vgetq_lane_u64::<1>;
    let operations: [unsafe fn(uint8x16_t, uint8x16_t) -> uint8x16_t; 3] =
        [vandq_u8, vorrq_u8, vceqq_u8];
    let bytes = [0u8, 1, 2, 3, 4, 5, 6, 7, 128, 129, 130, 131, 252, 253, 254, 255];
    unsafe {
        let vector = load(bytes.as_ptr());
        for operation in operations {
            println!("{}", lane(words(operation(vector, splat(128)))));
        }
        println!("{}", lane(words(apply(|b| vdupq_n_u8(b), 7))));
    }
}
