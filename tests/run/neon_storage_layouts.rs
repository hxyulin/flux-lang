#[cfg(target_arch = "aarch64")]
fn main() {
    use std::arch::aarch64::*;
    fn check<T: Copy>(size: usize, align: usize) {
        assert_eq!(std::mem::size_of::<T>(), size);
        assert_eq!(std::mem::align_of::<T>(), align);
    }
    check::<float32x2_t>(8, 8);
    check::<float32x4_t>(16, 16);
    check::<float64x1_t>(8, 8);
    check::<float64x2_t>(16, 16);
    check::<int16x4_t>(8, 8);
    check::<int64x1_t>(8, 8);
    check::<int64x2_t>(16, 16);
    check::<poly8x8_t>(8, 8);
    check::<poly8x16_t>(16, 16);
    check::<poly16x4_t>(8, 8);
    check::<poly16x8_t>(16, 16);
    check::<poly64x1_t>(8, 8);
    check::<poly64x2_t>(16, 16);
    check::<int8x8x2_t>(16, 8);
    check::<int8x8x3_t>(24, 8);
    check::<int8x8x4_t>(32, 8);
    check::<int8x16x2_t>(32, 16);
    check::<int8x16x3_t>(48, 16);
    check::<int8x16x4_t>(64, 16);
    check::<uint8x8x2_t>(16, 8);
    check::<uint8x8x3_t>(24, 8);
    check::<uint8x8x4_t>(32, 8);
    check::<uint8x16x2_t>(32, 16);
    check::<uint8x16x3_t>(48, 16);
    check::<uint8x16x4_t>(64, 16);
    check::<poly8x8x2_t>(16, 8);
    check::<poly8x8x3_t>(24, 8);
    check::<poly8x8x4_t>(32, 8);
    check::<poly8x16x2_t>(32, 16);
    check::<poly8x16x3_t>(48, 16);
    check::<poly8x16x4_t>(64, 16);
    check::<uint16x4x2_t>(16, 8);
    check::<uint16x4x3_t>(24, 8);
    check::<uint16x4x4_t>(32, 8);
    check::<float32x2x2_t>(16, 8);
    check::<float32x2x3_t>(24, 8);
    check::<float32x2x4_t>(32, 8);
    check::<float32x4x2_t>(32, 16);
    check::<float32x4x3_t>(48, 16);
    check::<float32x4x4_t>(64, 16);
    check::<float64x1x2_t>(16, 8);
    check::<float64x1x3_t>(24, 8);
    check::<float64x1x4_t>(32, 8);
    check::<float64x2x2_t>(32, 16);
    check::<float64x2x3_t>(48, 16);
    check::<float64x2x4_t>(64, 16);
    check::<int16x4x2_t>(16, 8);
    check::<int16x4x3_t>(24, 8);
    check::<int16x4x4_t>(32, 8);
    check::<int16x8x3_t>(48, 16);
    check::<int16x8x4_t>(64, 16);
    check::<int32x2x2_t>(16, 8);
    check::<int32x2x3_t>(24, 8);
    check::<int32x2x4_t>(32, 8);
    check::<int32x4x3_t>(48, 16);
    check::<int32x4x4_t>(64, 16);
    check::<int64x1x2_t>(16, 8);
    check::<int64x1x3_t>(24, 8);
    check::<int64x1x4_t>(32, 8);
    check::<int64x2x2_t>(32, 16);
    check::<int64x2x3_t>(48, 16);
    check::<int64x2x4_t>(64, 16);
    check::<poly16x4x2_t>(16, 8);
    check::<poly16x4x3_t>(24, 8);
    check::<poly16x4x4_t>(32, 8);
    check::<poly16x8x2_t>(32, 16);
    check::<poly16x8x3_t>(48, 16);
    check::<poly16x8x4_t>(64, 16);
    check::<poly64x1x2_t>(16, 8);
    check::<poly64x1x3_t>(24, 8);
    check::<poly64x1x4_t>(32, 8);
    check::<poly64x2x2_t>(32, 16);
    check::<poly64x2x3_t>(48, 16);
    check::<poly64x2x4_t>(64, 16);
    check::<uint16x8x3_t>(48, 16);
    check::<uint16x8x4_t>(64, 16);
    check::<uint32x2x2_t>(16, 8);
    check::<uint32x2x3_t>(24, 8);
    check::<uint32x2x4_t>(32, 8);
    check::<uint32x4x3_t>(48, 16);
    check::<uint32x4x4_t>(64, 16);
    check::<uint64x1x2_t>(16, 8);
    check::<uint64x1x3_t>(24, 8);
    check::<uint64x1x4_t>(32, 8);
    check::<uint64x2x2_t>(32, 16);
    check::<uint64x2x3_t>(48, 16);
    check::<uint64x2x4_t>(64, 16);
    let left = [i32::MIN, -1, 0, i32::MAX];
    let right = [1, -5, i32::MIN, -1];
    #[repr(align(16))]
    struct AlignedBytes([u8; 18]);
    let mut output = AlignedBytes([0xa5; 18]);
    // SAFETY: NEON is baseline on AArch64. Both inputs have four initialized
    // lanes, and the store writes sixteen bytes within the output array.
    unsafe {
        let difference = vsubq_s32(vld1q_s32(left.as_ptr()), vld1q_s32(right.as_ptr()));
        vst1q_s32(output.0.as_mut_ptr().add(1).cast::<i32>(), difference);
        let stored = output.0.as_ptr().add(1).cast::<[i32; 4]>().read_unaligned();
        assert_eq!(stored, [i32::MAX, 4, i32::MIN, i32::MIN]);
        let round_trip: [i32; 4] =
            std::mem::transmute(vld1q_s32(output.0.as_ptr().add(1).cast::<i32>()));
        assert_eq!(round_trip, stored);
    }
    assert_eq!([output.0[0], output.0[17]], [0xa5; 2]);
    println!("neon storage layouts and signed lanes passed");
}
#[cfg(not(target_arch = "aarch64"))]
fn main() {}
