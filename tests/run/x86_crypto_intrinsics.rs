#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86_64")]
fn carryless(a: u64, b: u64) -> [u64; 2] {
    let mut product = 0u128;
    for bit in 0..64 {
        if (b >> bit) & 1 != 0 {
            product ^= (a as u128) << bit;
        }
    }
    [product as u64, (product >> 64) as u64]
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,vpclmulqdq")]
unsafe fn fold<const IMM: i32>(a: __m512i, b: __m512i) {
    // SAFETY: the target features are enabled and every lane has initialized bits.
    unsafe {
        let result: [u64; 8] = core::mem::transmute(_mm512_clmulepi64_epi128::<IMM>(a, b));
        let a: [u64; 8] = core::mem::transmute(a);
        let b: [u64; 8] = core::mem::transmute(b);
        for lane in 0..4 {
            let expected = carryless(
                a[lane * 2 + (IMM as usize & 1)],
                b[lane * 2 + ((IMM as usize >> 4) & 1)],
            );
            assert_eq!(result[lane * 2], expected[0]);
            assert_eq!(result[lane * 2 + 1], expected[1]);
        }
        println!("{:?}", result);
    }
}

#[cfg(target_arch = "x86_64")]
fn main() {
    assert_eq!(core::mem::size_of::<__m512i>(), 64);
    assert_eq!(core::mem::align_of::<__m512i>(), 64);
    println!(
        "{} {} {} {} {}",
        is_x86_feature_detected!("avx512f"),
        is_x86_feature_detected!("vpclmulqdq"),
        is_x86_feature_detected!("pclmulqdq"),
        is_x86_feature_detected!("sha"),
        is_x86_feature_detected!("xsave")
    );
    let base = __cpuid(0);
    assert_eq!(base.eax, __cpuid_count(0, 0).eax);
    assert_eq!(_XCR_XFEATURE_ENABLED_MASK, 0);
    if is_x86_feature_detected!("pclmulqdq") {
        // SAFETY: hardware support was detected and all values are initialized.
        unsafe {
            let a = _mm_set_epi64x(-1, 0x123456789abcdef);
            let b = _mm_set_epi64x(i64::MIN, 0x76543210fedcba9);
            for result in [
                _mm_clmulepi64_si128::<0>(a, b),
                _mm_clmulepi64_si128::<1>(a, b),
                _mm_clmulepi64_si128::<16>(a, b),
                _mm_clmulepi64_si128::<17>(a, b),
                _mm_clmulepi64_si128::<255>(a, b),
            ] {
                let words: [u64; 2] = core::mem::transmute(result);
                println!("{:?}", words);
            }
        }
    }
    if is_x86_feature_detected!("vpclmulqdq") && is_x86_feature_detected!("avx2") {
        // SAFETY: the required features were detected.
        unsafe {
            let a = _mm256_set_epi64x(-1, 19, i64::MIN, 37);
            let b = _mm256_set_epi64x(7, -9, 11, -13);
            for result in [
                _mm256_clmulepi64_epi128::<0>(a, b),
                _mm256_clmulepi64_epi128::<1>(a, b),
                _mm256_clmulepi64_epi128::<16>(a, b),
                _mm256_clmulepi64_epi128::<17>(a, b),
            ] {
                let words: [u64; 4] = core::mem::transmute(result);
                println!("{:?}", words);
            }
        }
    }
    if is_x86_feature_detected!("avx512f") && is_x86_feature_detected!("vpclmulqdq") {
        // SAFETY: the required features were detected; all buffers contain 64 valid bytes.
        unsafe {
            let a = _mm512_set_epi64(-1, 2, i64::MIN, 4, 5, -6, 7, 0x123456789abcdef);
            let b = _mm512_set_epi64(11, -12, 13, -14, 15, 16, -17, 18);
            assert_eq!((&a as *const __m512i as usize) % 64, 0);
            assert_eq!((&b as *const __m512i as usize) % 64, 0);
            println!("{:?}", a);
            fold::<0>(a, b);
            fold::<1>(a, b);
            fold::<16>(a, b);
            fold::<17>(a, b);
            fold::<255>(a, b);
            let c = _mm512_xor_si512(a, b);
            let words: [u64; 8] = core::mem::transmute(c);
            println!("{:?}", words);
            let mut buffer = [0xa5u8; 66];
            _mm512_storeu_si512(buffer.as_mut_ptr().add(1).cast(), c);
            assert_eq!(buffer[0], 0xa5);
            assert_eq!(buffer[65], 0xa5);
            let loaded = _mm512_loadu_si512(buffer.as_ptr().add(1).cast());
            let loaded: [u64; 8] = core::mem::transmute(loaded);
            assert_eq!(loaded, words);
            for quarter in [
                _mm512_extracti32x4_epi32::<0>(a),
                _mm512_extracti32x4_epi32::<1>(a),
                _mm512_extracti32x4_epi32::<2>(a),
                _mm512_extracti32x4_epi32::<3>(a),
            ] {
                let words: [u64; 2] = core::mem::transmute(quarter);
                println!("{:?}", words);
            }
            let low = _mm512_castsi512_si128(a);
            let widened = _mm512_castsi128_si512(low);
            let low: [u64; 2] = core::mem::transmute(low);
            let roundtrip: [u64; 2] = core::mem::transmute(_mm512_castsi512_si128(widened));
            assert_eq!(low, roundtrip);
            let zero: [u64; 8] = core::mem::transmute(_mm512_setzero_si512());
            assert_eq!(zero, [0u64; 8]);
        }
    }
    if is_x86_feature_detected!("sha") {
        // SAFETY: SHA support was detected; these operations have no memory requirements.
        unsafe {
            let a = _mm_set_epi32(1, -2, 3, -4);
            let b = _mm_set_epi32(-5, 6, -7, 8);
            let k = _mm_set_epi32(19, 20, 21, 22);
            for result in [
                _mm_sha256msg1_epu32(a, b),
                _mm_sha256msg2_epu32(a, b),
                _mm_sha256rnds2_epu32(a, b, k),
                _mm_sha256rnds2_epu32(b, a, k),
            ] {
                let words: [u32; 4] = core::mem::transmute(result);
                println!("{:?}", words);
            }
        }
    }
    println!("x86 crypto complete");
}

#[cfg(not(target_arch = "x86_64"))]
fn main() {}
