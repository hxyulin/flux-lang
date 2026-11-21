use core::arch::aarch64::*;

#[target_feature(enable = "neon")]
fn signed_arithmetic() {
    let data = [i16::MIN, -8, -3, 4, 5, -6, 7, i16::MAX];
    // SAFETY: data contains eight initialized halfwords. All vector
    // transmutes preserve the bytes, and the immediates are in range.
    unsafe {
        let a = vld1q_s16(data.as_ptr());
        let b = vdupq_n_s16(3);
        let add: [i16; 8] = core::mem::transmute(vaddq_s16(a, b));
        let sub: [i16; 8] = core::mem::transmute(vsubq_s16(a, b));
        let mul: [i16; 8] = core::mem::transmute(vmulq_s16(a, b));
        let shift: [i16; 8] = core::mem::transmute(vshrq_n_s16::<16>(a));
        for i in 0..8 {
            assert_eq!(add[i], data[i].wrapping_add(3));
            assert_eq!(sub[i], data[i].wrapping_sub(3));
            assert_eq!(mul[i], data[i].wrapping_mul(3));
            assert_eq!(shift[i], data[i] >> 15);
        }
        let raw: [i16; 4] = core::mem::transmute(vcreate_s16(0x8000_ffff_0002_0001));
        assert_eq!(raw, [1, 2, -1, i16::MIN]);
        let acc = vdupq_n_s32(i32::MAX - 3);
        let low: [i32; 4] = core::mem::transmute(vmlal_laneq_s16::<1>(acc, vget_low_s16(a), a));
        let high: [i32; 4] = core::mem::transmute(vmlal_high_laneq_s16::<7>(acc, a, a));
        for i in 0..4 {
            assert_eq!(low[i], (i32::MAX - 3).wrapping_add(data[i] as i32 * -8));
            assert_eq!(
                high[i],
                (i32::MAX - 3).wrapping_add(data[i + 4] as i32 * i16::MAX as i32)
            );
        }
        let input = [-100, 201, 131_072, i32::MAX];
        let words = vld1q_s32(input.as_ptr());
        let packed: [u16; 4] = core::mem::transmute(vqshrun_n_s32::<1>(words));
        assert_eq!(packed, [0, 100, u16::MAX, u16::MAX]);
        let bytes: [u8; 8] = core::mem::transmute(vqmovn_u16(vcombine_u16(
            vqshrun_n_s32::<1>(words),
            vqshrun_n_s32::<1>(words),
        )));
        assert_eq!(bytes, [0, 100, 255, 255, 0, 100, 255, 255]);
        let doubled: [i32; 4] = core::mem::transmute(vaddq_s32(words, words));
        let multiplied: [i32; 4] = core::mem::transmute(vmulq_s32(words, vdupq_n_s32(-3)));
        let bits: [i32; 4] = core::mem::transmute(vorrq_s32(words, vdupq_n_s32(7)));
        let signed_shift: [i32; 4] = core::mem::transmute(vshrq_n_s32::<32>(words));
        for i in 0..4 {
            assert_eq!(doubled[i], input[i].wrapping_add(input[i]));
            assert_eq!(multiplied[i], input[i].wrapping_mul(-3));
            assert_eq!(bits[i], input[i] | 7);
            assert_eq!(signed_shift[i], input[i] >> 31);
        }
        assert_eq!(vmaxvq_u32(vreinterpretq_u32_s32(words)), (-100i32) as u32);
        println!("{low:?} {high:?} {packed:?}");
    }
}

#[target_feature(enable = "neon")]
fn permutation_and_stores() {
    let a = [1i16, 2, 3, 4, 5, 6, 7, 8, 21, 22, 23, 24, 25, 26, 27, 28];
    let words = [1i32, 2, 3, 4, -1, -2, -3, -4];
    let mut reversed = [0i16; 18];
    let mut rgb = [0; 50];
    let mut rgba = [0; 66];
    // SAFETY: the loads and stores each have space for the given vector
    // count. The vector casts preserve the byte representation.
    unsafe {
        let pair = vld1q_s16_x2(a.as_ptr());
        vst1q_s16_x2(reversed.as_mut_ptr().add(1), int16x8x2_t(pair.1, pair.0));
        let zip_low: [i16; 8] = core::mem::transmute(vzip1q_s16(pair.0, pair.1));
        let zip_high: [i16; 8] = core::mem::transmute(vzip2q_s16(pair.0, pair.1));
        assert_eq!(zip_low, [1, 21, 2, 22, 3, 23, 4, 24]);
        assert_eq!(zip_high, [5, 25, 6, 26, 7, 27, 8, 28]);

        let pair = vld1q_s32_x2(words.as_ptr());
        let transposed = vtrnq_s32(pair.0, pair.1);
        let even: [i32; 4] = core::mem::transmute(transposed.0);
        let odd: [i32; 4] = core::mem::transmute(transposed.1);
        assert_eq!(even, [1, -1, 3, -3]);
        assert_eq!(odd, [2, -2, 4, -4]);
        let a = vreinterpretq_s64_s32(pair.0);
        let b = vreinterpretq_s64_s32(pair.1);
        let low: [i32; 4] = core::mem::transmute(vtrn1q_s64(a, b));
        let high: [i32; 4] = core::mem::transmute(vtrn2q_s64(a, b));
        assert_eq!(low, [1, 2, -1, -2]);
        assert_eq!(high, [3, 4, -3, -4]);

        let r = vdupq_n_u8(7);
        let g = vdupq_n_u8(11);
        let b = vdupq_n_u8(13);
        vst3q_u8(rgb.as_mut_ptr().add(1), uint8x16x3_t(r, g, b));
        vst4q_u8(
            rgba.as_mut_ptr().add(1),
            uint8x16x4_t(r, g, b, vdupq_n_u8(17)),
        );
    }
    assert_eq!(
        reversed,
        [0, 21, 22, 23, 24, 25, 26, 27, 28, 1, 2, 3, 4, 5, 6, 7, 8, 0]
    );
    assert_eq!([rgb[0], rgb[49], rgba[0], rgba[65]], [0; 4]);
    for i in 0..16 {
        let j = i * 3 + 1;
        let k = i * 4 + 1;
        assert_eq!([rgb[j], rgb[j + 1], rgb[j + 2]], [7, 11, 13]);
        assert_eq!(
            [rgba[k], rgba[k + 1], rgba[k + 2], rgba[k + 3]],
            [7, 11, 13, 17]
        );
    }
    println!("{reversed:?}");
}

fn main() {
    // SAFETY: NEON is available on the AArch64 hosts running this test.
    unsafe {
        signed_arithmetic();
        permutation_and_stores();
    }
}
