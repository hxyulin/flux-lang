use core::arch::aarch64::*;
use core::mem::transmute;

fn main() {
    // SAFETY: this AArch64 target supplies NEON. All vector bit patterns are
    // initialized integers and each immediate lies within its intrinsic's range.
    unsafe {
        let words = [0x80000001u32, 0xffffffff, 0x12345678, 0x90abcdef];
        let a: uint32x4_t = transmute(words);
        let b: uint32x4_t = transmute([7u32, 11, 13, 17]);
        let halves = vreinterpretq_u16_u32(a);
        let round: [u32; 4] = transmute(vreinterpretq_u32_u16(halves));
        assert_eq!(round, words);
        let wide = vreinterpretq_u64_u32(a);
        let round: [u32; 4] = transmute(vreinterpretq_u32_u64(wide));
        assert_eq!(round, words);
        let bytes = vreinterpretq_u8_u32(a);
        let raw: [u8; 16] = transmute(bytes);
        let round: [u32; 4] = transmute(vreinterpretq_u32_u8(bytes));
        assert_eq!(round, words);
        let reversed: [u16; 8] = transmute(vrev32q_u16(halves));
        let original: [u16; 8] = transmute(halves);
        for i in 0..8 { assert_eq!(reversed[i], original[i ^ 1]); }
        let ored: [u32; 4] = transmute(vorrq_u32(a, b));
        let xored: [u32; 4] = transmute(veorq_u32(a, b));
        let rhs = [7u32, 11, 13, 17];
        for i in 0..4 {
            assert_eq!(ored[i], words[i] | rhs[i]);
            assert_eq!(xored[i], words[i] ^ rhs[i]);
        }
        let left: [u32; 4] = transmute(vshlq_n_u32::<31>(a));
        let unchanged: [u32; 4] = transmute(vshlq_n_u32(a, 0));
        let right: [u32; 4] = transmute(vshrq_n_u32::<1>(a));
        let zero: [u32; 4] = transmute(vshrq_n_u32::<32>(a));
        assert_eq!(unchanged, words);
        assert_eq!(zero, [0; 4]);
        for i in 0..4 {
            assert_eq!(left[i], words[i] << 31);
            assert_eq!(right[i], words[i] >> 1);
        }
        let shifted: [u8; 16] = transmute(vshrq_n_u8::<1>(bytes));
        let zero: [u8; 16] = transmute(vshrq_n_u8::<8>(bytes));
        assert_eq!(zero, [0; 16]);
        for i in 0..16 { assert_eq!(shifted[i], raw[i] >> 1); }
        let first: [u8; 16] = transmute(vextq_u8::<0>(bytes, bytes));
        let last: [u8; 16] = transmute(vextq_u8::<15>(bytes, bytes));
        assert_eq!(first, raw);
        for i in 0..16 { assert_eq!(last[i], raw[(i + 15) % 16]); }
        let joined: [u32; 4] = transmute(vextq_u32::<3>(a, b));
        let first: [u32; 4] = transmute(vextq_u32::<0>(a, b));
        assert_eq!(first, words);
        assert_eq!(joined, [words[3], 7, 11, 13]);
        println!("{} {} {}", round[2], last[0], joined[3]);
    }
}
