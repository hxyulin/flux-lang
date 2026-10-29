use core::arch::aarch64::*;
use core::mem::{align_of, size_of, transmute};

fn main() {
    assert_eq!((size_of::<int32x2_t>(), align_of::<int32x2_t>()), (8, 8));
    assert_eq!((size_of::<int32x4_t>(), align_of::<int32x4_t>()), (16, 16));
    assert_eq!((size_of::<int16x8_t>(), align_of::<int16x8_t>()), (16, 16));
    let mut digest = 0u64;
    unsafe {
        let bits = 0x807f060504030201u64;
        let signed = vreinterpret_s32_u64(vcreate_u64(bits));
        let words = vreinterpret_u32_s32(signed);
        let raw: [u32; 2] = transmute(words);
        assert_eq!(raw, [0x04030201, 0x807f0605]);
        let half: [u16; 4] = transmute(vreinterpret_u16_s32(signed));
        assert_eq!(half, [0x0201, 0x0403, 0x0605, 0x807f]);
        let shifted: [u32; 4] = transmute(vshll_n_u16::<16>(vreinterpret_u16_s32(signed)));
        assert_eq!(shifted, [0x02010000, 0x04030000, 0x06050000, 0x807f0000]);
        let legacy_left: [u32; 4] = transmute(vshll_n_u16(vreinterpret_u16_s32(signed), 0));
        assert_eq!(legacy_left, [0x0201, 0x0403, 0x0605, 0x807f]);
        let legacy_right: [u32; 2] = transmute(vshr_n_u32(words, 32));
        assert_eq!(legacy_right, [0, 0]);
        let zero: [u32; 2] = transmute(vshr_n_u32::<32>(words));
        assert_eq!(zero, [0, 0]);
        let shr: [u32; 2] = transmute(vshr_n_u32::<1>(words));
        assert_eq!(shr, [raw[0] >> 1, raw[1] >> 1]);
        let joined: [i32; 4] = transmute(vcombine_s32(signed, vdup_n_s32(-7)));
        assert_eq!(joined, [raw[0] as i32, raw[1] as i32, -7, -7]);
        let q: uint32x4_t = transmute([0x807f0605u32, 0xffffffff, 0x01234567, 0x89abcdef]);
        let sq = vreinterpretq_s32_u32(q);
        let halves = vreinterpretq_s16_s32(sq);
        let byte_vec = vreinterpretq_u8_s16(halves);
        let byte_array: [u8; 16] = transmute(byte_vec);
        let q_halves = vreinterpretq_u16_s8(vreinterpretq_s8_u8(byte_vec));
        let restored: [u16; 8] = transmute(q_halves);
        let expected: [u16; 8] = transmute([0x807f0605u32, 0xffffffff, 0x01234567, 0x89abcdef]);
        assert_eq!(restored, expected);
        let copy: [i16; 8] = transmute(halves);
        assert_eq!(copy[0] as u16, restored[0]);
        let low: [u8; 8] = transmute(vget_low_u8(byte_vec));
        for i in 0..8 {
            assert_eq!(low[i], byte_array[i]);
        }
        let mask: [u8; 16] = transmute(vcgtzq_s8(vreinterpretq_s8_u8(byte_vec)));
        for i in 0..16 {
            assert_eq!(mask[i], if byte_array[i] as i8 > 0 { 255 } else { 0 });
        }
        let signed_splat: [i8; 16] = transmute(vdupq_n_s8(-9));
        assert_eq!(signed_splat, [-9; 16]);
        let mut table = [0u8; 16];
        for i in 0..16 {
            table[i] = (i * 7 + 3) as u8;
        }
        let table_vec: uint8x16_t = transmute(table);
        for block in 0..16 {
            let mut indices = [0u8; 16];
            for i in 0..16 {
                indices[i] = (block * 16 + i) as u8;
            }
            let result = vqtbl1q_u8(table_vec, transmute(indices));
            let values: [u8; 16] = transmute(result);
            let mut stored = [0xa5u8; 18];
            vst1q_u8(stored.as_mut_ptr().add(1), result);
            assert_eq!(stored[0], 0xa5);
            assert_eq!(stored[17], 0xa5);
            for i in 0..16 {
                let expected = if indices[i] < 16 {
                    table[indices[i] as usize]
                } else {
                    0
                };
                assert_eq!(values[i], expected);
                assert_eq!(stored[i + 1], expected);
                digest = digest.wrapping_add(values[i] as u64);
            }
        }
        let original: uint64x2_t = transmute([17u64, 23]);
        let legacy_lane: [u64; 2] = transmute(vsetq_lane_u64(9, original, 0));
        assert_eq!(legacy_lane, [9, 23]);
        let setter: unsafe fn(u64, uint64x2_t) -> uint64x2_t = vsetq_lane_u64::<1>;
        let updated: [u64; 2] = transmute(setter(41, original));
        assert_eq!(updated, [17, 41]);
        let untouched: [u64; 2] = transmute(original);
        assert_eq!(untouched, [17, 23]);
        let bytes: [u8; 16] = transmute(vreinterpretq_u8_u64(original));
        assert_eq!(bytes[0], 17);
        assert_eq!(bytes[8], 23);
        let mut state = 0x123456789abcdef0u64;
        for iteration in 0..512 {
            let mut a16 = [0i16; 8];
            let mut b16 = [0i16; 8];
            for i in 0..8 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                a16[i] = state as i16;
                b16[i] = (state >> 32) as i16;
            }
            let mut factor = state as i16;
            if iteration == 0 {
                a16 = [i16::MIN, i16::MAX, -1, 0, 1, -32767, 123, -456];
                factor = i16::MIN;
            }
            let a: int16x8_t = transmute(a16);
            let b: int16x8_t = transmute(b16);
            let mla: [i16; 8] = transmute(vmlaq_n_s16(a, b, factor));
            let high: [i16; 8] = transmute(vqdmulhq_n_s16(a, factor));
            let add: [u16; 8] = transmute(vaddq_u16(transmute(a16), transmute(b16)));
            for i in 0..8 {
                assert_eq!(mla[i], a16[i].wrapping_add(b16[i].wrapping_mul(factor)));
                let expected = if a16[i] == i16::MIN && factor == i16::MIN {
                    i16::MAX
                } else {
                    ((a16[i] as i32 * factor as i32) >> 15) as i16
                };
                assert_eq!(high[i], expected);
                assert_eq!(add[i], (a16[i] as u16).wrapping_add(b16[i] as u16));
                digest = digest.wrapping_mul(31).wrapping_add(high[i] as u16 as u64);
            }
            let mut a32 = [state as i32, (state >> 32) as i32, -1, i32::MIN];
            let b32 = [17i32, -23, i32::MAX, i32::MIN];
            let mut factor32 = state as i32;
            if iteration == 0 {
                a32 = [i32::MIN, i32::MAX, -1, 0];
                factor32 = i32::MIN;
            }
            let a: int32x4_t = transmute(a32);
            let b: int32x4_t = transmute(b32);
            let mla: [i32; 4] = transmute(vmlaq_n_s32(a, b, factor32));
            let high: [i32; 4] = transmute(vqdmulhq_n_s32(a, factor32));
            let low: int32x2_t = transmute([a32[0], a32[1]]);
            let low_b: int32x2_t = transmute([b32[0], b32[1]]);
            let mla_low: [i32; 2] = transmute(vmla_n_s32(low, low_b, factor32));
            let high_low: [i32; 2] = transmute(vqdmulh_n_s32(low, factor32));
            for i in 0..4 {
                assert_eq!(mla[i], a32[i].wrapping_add(b32[i].wrapping_mul(factor32)));
                let expected = if a32[i] == i32::MIN && factor32 == i32::MIN {
                    i32::MAX
                } else {
                    ((a32[i] as i64 * factor32 as i64) >> 31) as i32
                };
                assert_eq!(high[i], expected);
                if i < 2 {
                    assert_eq!(mla_low[i], mla[i]);
                    assert_eq!(high_low[i], high[i]);
                }
                digest = digest.wrapping_mul(31).wrapping_add(high[i] as u32 as u64);
            }
        }
    }
    println!("neon zmij {}", digest);
}
