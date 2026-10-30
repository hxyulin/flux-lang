#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(target_arch = "x86_64")]
unsafe fn bytes(a: __m128i) -> [u8; 16] {
    unsafe { std::mem::transmute(a) }
}
#[cfg(target_arch = "x86_64")]
unsafe fn wide(a: __m256i) -> [u8; 32] {
    unsafe { std::mem::transmute(a) }
}

#[cfg(target_arch = "x86_64")]
fn main() {
    assert_eq!(std::mem::size_of::<__m128i>(), 16);
    assert_eq!(std::mem::align_of::<__m128i>(), 16);
    assert_eq!(std::mem::size_of::<__m256i>(), 32);
    assert_eq!(std::mem::align_of::<__m256i>(), 32);
    assert!(is_x86_feature_detected!("sse2"));
    let input = [
        0u8, 128, 255, 127, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13,
    ];
    println!(
        "{} {} {} {} {} {} {} {}",
        is_x86_feature_detected!("sse"),
        is_x86_feature_detected!("sse2"),
        is_x86_feature_detected!("sse3"),
        is_x86_feature_detected!("ssse3"),
        is_x86_feature_detected!("sse4.1"),
        is_x86_feature_detected!("sse4.2"),
        is_x86_feature_detected!("avx"),
        is_x86_feature_detected!("avx2")
    );
    unsafe {
        let cpu = __cpuid_count(0, 0);
        println!("{} {} {} {}", cpu.eax, cpu.ebx, cpu.ecx, cpu.edx);
        if __cpuid_count(1, 0).ecx & (1 << 27) != 0 {
            println!("{}", _xgetbv(0));
        }
        println!("{:?}", _mm_set_epi64x(-1, i64::MIN));
        let a = _mm_loadu_si128(input.as_ptr().add(1).cast());
        let b = _mm_set_epi32(-1, 0x12345678, i32::MIN, 0x7fff8000);
        let mut aligned = [a];
        _mm_store_si128(aligned.as_mut_ptr(), a);
        assert_eq!(bytes(_mm_load_si128(aligned.as_ptr())), bytes(a));
        println!("{:?}", bytes(_mm_setr_epi32(1, 2, 3, 4)));
        println!("{:?}", bytes(_mm_set_epi64x(-1, i64::MIN)));
        println!("{}", _mm_cvtsi128_si64(a));
        println!("{:?}", bytes(_mm_add_epi8(a, b)));
        println!("{:?}", bytes(_mm_cmpeq_epi16(a, b)));
        println!("{:?}", bytes(_mm_cmpeq_epi32(a, b)));
        println!("{:?}", bytes(_mm_cmpgt_epi16(a, b)));
        println!("{:?}", bytes(_mm_cmpgt_epi32(a, b)));
        println!("{:?}", bytes(_mm_packs_epi16(a, b)));
        println!("{:?}", bytes(_mm_packs_epi32(a, b)));
        println!("{:?}", bytes(_mm_cvtsi32_si128(-1)));
        println!("{:?}", bytes(_mm_set1_epi16(-32768)));
        println!("{:?}", bytes(_mm_set1_epi32(i32::MIN)));
        println!(
            "{:?}",
            bytes(_mm_setr_epi8(
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, -1, -2, -3, -4, -5, -6
            ))
        );
        println!("{:?}", bytes(_mm_srli_si128::<0>(a)));
        println!("{:?}", bytes(_mm_srli_si128::<7>(a)));
        println!("{:?}", bytes(_mm_srli_si128::<255>(a)));
        _mm_prefetch::<_MM_HINT_T0>(input.as_ptr().cast());
        _mm_prefetch::<_MM_HINT_T1>(input.as_ptr().cast());
        _mm_prefetch::<_MM_HINT_T2>(input.as_ptr().cast());
        _mm_prefetch::<_MM_HINT_NTA>(input.as_ptr().cast());
        println!("{:?}", bytes(_mm_add_epi32(a, b)));
        println!("{:?}", bytes(_mm_add_epi64(a, b)));
        println!("{:?}", bytes(_mm_sub_epi16(a, b)));
        println!("{:?}", bytes(_mm_mullo_epi16(a, b)));
        println!("{:?}", bytes(_mm_mulhi_epu16(a, b)));
        println!("{:?}", bytes(_mm_mul_epu32(a, b)));
        println!("{:?}", bytes(_mm_and_si128(a, b)));
        println!("{:?}", bytes(_mm_or_si128(a, b)));
        println!("{:?}", bytes(_mm_xor_si128(a, b)));
        println!("{:?}", bytes(_mm_cmpeq_epi8(a, b)));
        println!("{:?}", bytes(_mm_cmpgt_epi8(a, b)));
        println!(
            "{} {}",
            _mm_movemask_epi8(a),
            _mm_movemask_epi8(_mm_set1_epi8(-1))
        );
        println!("{:?}", bytes(_mm_shuffle_epi32::<0x1b>(a)));
        println!("{:?}", bytes(_mm_slli_epi16::<0>(a)));
        println!("{:?}", bytes(_mm_slli_epi16::<16>(a)));
        println!("{:?}", bytes(_mm_slli_epi32::<31>(a)));
        println!("{:?}", bytes(_mm_slli_epi64::<255>(a)));
        println!("{:?}", bytes(_mm_srli_epi16::<15>(a)));
        println!("{:?}", bytes(_mm_srli_epi32::<32>(a)));
        println!("{:?}", bytes(_mm_srli_epi64::<63>(a)));
        let mut stored = [0u8; 18];
        _mm_storeu_si128(stored.as_mut_ptr().add(1).cast(), a);
        assert_eq!(stored[0], 0);
        assert_eq!(stored[17], 0);
        println!("{:?}", stored);
        if is_x86_feature_detected!("ssse3") {
            println!("{:?}", bytes(_mm_shuffle_epi8(a, b)));
            println!("{:?}", bytes(_mm_alignr_epi8::<0>(a, b)));
            println!("{:?}", bytes(_mm_alignr_epi8::<7>(a, b)));
            println!("{:?}", bytes(_mm_alignr_epi8::<17>(a, b)));
            println!("{:?}", bytes(_mm_alignr_epi8::<32>(a, b)));
        }
        if is_x86_feature_detected!("sse4.1") {
            println!(
                "{} {}",
                _mm_extract_epi32::<0>(a),
                _mm_extract_epi32::<3>(a)
            );
            println!("{:?}", bytes(_mm_blend_epi16::<0xa5>(a, b)));
        }
        if is_x86_feature_detected!("avx2") {
            println!("{:?}", _mm256_set_epi64x(-1, 2, -3, 4));
            assert_eq!(
                bytes(_mm256_castsi256_si128(_mm256_castsi128_si256(a))),
                bytes(a)
            );
            let a = _mm256_set_m128i(a, b);
            let b = _mm256_set_epi64x(-1, 0, i64::MIN, 0x123456789abcdef);
            let aligned = [a];
            assert_eq!(wide(_mm256_load_si256(aligned.as_ptr())), wide(a));
            assert_eq!(wide(_mm256_loadu_si256(aligned.as_ptr())), wide(a));
            println!("{:?}", wide(_mm256_set_epi32(1, 2, 3, 4, 5, 6, 7, 8)));
            println!("{:?}", wide(_mm256_add_epi32(a, b)));
            println!("{:?}", wide(_mm256_add_epi64(a, b)));
            println!("{:?}", wide(_mm256_sub_epi16(a, b)));
            println!("{:?}", wide(_mm256_mullo_epi16(a, b)));
            println!("{:?}", wide(_mm256_and_si256(a, b)));
            println!("{:?}", wide(_mm256_or_si256(a, b)));
            println!("{:?}", wide(_mm256_xor_si256(a, b)));
            println!("{:?}", wide(_mm256_cmpeq_epi8(a, b)));
            println!("{:?}", wide(_mm256_cmpgt_epi8(a, b)));
            println!(
                "{} {}",
                _mm256_movemask_epi8(a),
                _mm256_movemask_epi8(_mm256_set1_epi8(-1))
            );
            println!("{:?}", wide(_mm256_shuffle_epi8(a, b)));
            println!("{:?}", wide(_mm256_alignr_epi8::<7>(a, b)));
            println!("{:?}", wide(_mm256_alignr_epi8::<17>(a, b)));
            println!("{:?}", wide(_mm256_alignr_epi8::<255>(a, b)));
            println!("{:?}", wide(_mm256_slli_epi16::<16>(a)));
            println!("{:?}", wide(_mm256_slli_epi32::<3>(a)));
            println!("{:?}", wide(_mm256_slli_epi64::<63>(a)));
            println!("{:?}", wide(_mm256_srli_epi16::<0>(a)));
            println!("{:?}", wide(_mm256_srli_epi32::<255>(a)));
            println!("{:?}", wide(_mm256_srli_epi64::<17>(a)));
            println!("{:?}", bytes(_mm256_extracti128_si256::<1>(a)));
            println!(
                "{:?}",
                wide(_mm256_insertf128_si256::<0>(a, _mm_setzero_si128()))
            );
            println!("{:?}", wide(_mm256_setzero_si256()));
        }
    }
}
#[cfg(not(target_arch = "x86_64"))]
fn main() {}
