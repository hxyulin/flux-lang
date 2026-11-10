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
    println!("neon storage layouts passed");
}
#[cfg(not(target_arch = "aarch64"))]
fn main() {}
