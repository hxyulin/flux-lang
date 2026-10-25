use core::arch::aarch64::*;

unsafe fn mask(value: uint8x8_t) -> u64 {
    unsafe { vget_lane_u64::<0>(vreinterpret_u64_u8(value)) }
}
fn main() {
    let mut digest = 0u64;
    for start in 0..256u16 {
        let mut input = [0u8; 17];
        for i in 0..17 { input[i] = start.wrapping_add(i as u16 * 31) as u8; }
        for offset in 0..8 {
            unsafe {
                let value = vld1_u8(input.as_ptr().add(offset));
                let tag = vdup_n_u8(start as u8);
                let eq = vceq_u8(value, tag);
                let negative = vcltz_s8(vreinterpret_s8_u8(value));
                let positive = vcgez_s8(vreinterpret_s8_u8(value));
                let converted = vorr_u8(negative, vdup_n_u8(128));
                let mut output = [71u8; 12];
                vst1_u8(output.as_mut_ptr().add(2), converted);
                assert_eq!(output[0], 71); assert_eq!(output[1], 71);
                assert_eq!(output[10], 71); assert_eq!(output[11], 71);
                for i in 0..8 { assert_eq!(output[i + 2], if input[i + offset] >= 128 { 255 } else { 128 }); }
                digest = digest.rotate_left(7) ^ mask(eq) ^ mask(negative).rotate_left(17) ^ mask(positive).rotate_left(29);
            }
        }
    }
    let load: unsafe fn(*const u8) -> uint8x8_t = vld1_u8;
    let store: unsafe fn(*mut u8, uint8x8_t) = vst1_u8;
    let equal: unsafe fn(uint8x8_t, uint8x8_t) -> uint8x8_t = vceq_u8;
    let a = [0, 127, 128, 255, 19, 23, 31, 0];
    let mut b = [0u8; 8];
    unsafe { store(b.as_mut_ptr(), equal(load(a.as_ptr()), load(a.as_ptr()))); }
    println!("{} {} {} {}", digest, b[0], b[3], std::mem::size_of::<int8x8_t>());
}
