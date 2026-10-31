//! Integer SSE, AVX and AVX-512 intrinsics used by the compiler dependency graph.
//!
//! Values retain Rust's vector size and alignment across ordinary Rust calls.

/// 16 packed bytes with native x86 vector alignment.
#[allow(non_camel_case_types)]
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct __m128i(pub(crate) [u8; 16]);

/// 32 packed bytes with native x86 vector alignment.
#[allow(non_camel_case_types)]
#[repr(C, align(32))]
#[derive(Clone, Copy)]
pub struct __m256i(pub(crate) [u8; 32]);

/// Native _mm_and_si128 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_and_si128(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_and_si128(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_and_si128(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_or_si128 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_or_si128(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_or_si128(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_or_si128(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_xor_si128 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_xor_si128(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_xor_si128(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_xor_si128(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_add_epi32 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_add_epi32(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_add_epi32(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_add_epi32(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_add_epi64 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_add_epi64(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_add_epi64(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_add_epi64(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_sub_epi16 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_sub_epi16(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_sub_epi16(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_sub_epi16(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_mullo_epi16 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_mullo_epi16(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_mullo_epi16(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_mullo_epi16(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_cmpeq_epi8 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_cmpeq_epi8(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_cmpeq_epi8(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_cmpeq_epi8(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_cmpgt_epi8 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_cmpgt_epi8(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_cmpgt_epi8(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_cmpgt_epi8(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_loadu_si128 operation.
///
/// # Safety
/// The CPU must support sse2. The pointer must address 16 readable bytes.
#[target_feature(enable = "sse2")]
pub unsafe fn _mm_loadu_si128(p: *const __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_loadu_si128(p) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_loadu_si128(p: *const __m128i) -> __m128i {
    loop {}
}

/// Native _mm_load_si128 operation.
///
/// # Safety
/// The CPU must support sse2. The pointer must address 16 readable bytes aligned to 16 bytes.
#[target_feature(enable = "sse2")]
pub unsafe fn _mm_load_si128(p: *const __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_load_si128(p) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_load_si128(p: *const __m128i) -> __m128i {
    loop {}
}

/// Native _mm_movemask_epi8 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_movemask_epi8(a: __m128i) -> i32 {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_movemask_epi8(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_movemask_epi8(a: __m128i) -> i32 {
    loop {}
}

/// Native _mm_shuffle_epi8 operation.
///
/// # Safety
/// The CPU must support ssse3.
#[target_feature(enable = "ssse3")]
pub fn _mm_shuffle_epi8(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_shuffle_epi8(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_shuffle_epi8(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_slli_epi16 operation.
///
/// # Safety
/// The CPU must support sse2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse2")]
pub fn _mm_slli_epi16<const IMM8: i32>(a: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_slli_epi16::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_slli_epi16<const IMM8: i32>(a: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_srli_epi16 operation.
///
/// # Safety
/// The CPU must support sse2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse2")]
pub fn _mm_srli_epi16<const IMM8: i32>(a: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_srli_epi16::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_srli_epi16<const IMM8: i32>(a: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_slli_epi32 operation.
///
/// # Safety
/// The CPU must support sse2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse2")]
pub fn _mm_slli_epi32<const IMM8: i32>(a: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_slli_epi32::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_slli_epi32<const IMM8: i32>(a: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_srli_epi32 operation.
///
/// # Safety
/// The CPU must support sse2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse2")]
pub fn _mm_srli_epi32<const IMM8: i32>(a: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_srli_epi32::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_srli_epi32<const IMM8: i32>(a: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_slli_epi64 operation.
///
/// # Safety
/// The CPU must support sse2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse2")]
pub fn _mm_slli_epi64<const IMM8: i32>(a: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_slli_epi64::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_slli_epi64<const IMM8: i32>(a: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_srli_epi64 operation.
///
/// # Safety
/// The CPU must support sse2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse2")]
pub fn _mm_srli_epi64<const IMM8: i32>(a: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_srli_epi64::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_srli_epi64<const IMM8: i32>(a: __m128i) -> __m128i {
    loop {}
}

/// Native _mm256_and_si256 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_and_si256(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_and_si256(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_and_si256(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_or_si256 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_or_si256(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_or_si256(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_or_si256(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_xor_si256 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_xor_si256(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_xor_si256(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_xor_si256(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_add_epi32 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_add_epi32(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_add_epi32(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_add_epi32(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_add_epi64 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_add_epi64(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_add_epi64(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_add_epi64(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_sub_epi16 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_sub_epi16(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_sub_epi16(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_sub_epi16(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_mullo_epi16 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_mullo_epi16(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_mullo_epi16(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_mullo_epi16(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_cmpeq_epi8 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_cmpeq_epi8(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_cmpeq_epi8(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_cmpeq_epi8(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_cmpgt_epi8 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_cmpgt_epi8(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_cmpgt_epi8(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_cmpgt_epi8(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_loadu_si256 operation.
///
/// # Safety
/// The CPU must support avx. The pointer must address 32 readable bytes.
#[target_feature(enable = "avx")]
pub unsafe fn _mm256_loadu_si256(p: *const __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_loadu_si256(p) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_loadu_si256(p: *const __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_load_si256 operation.
///
/// # Safety
/// The CPU must support avx. The pointer must address 32 readable bytes aligned to 32 bytes.
#[target_feature(enable = "avx")]
pub unsafe fn _mm256_load_si256(p: *const __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_load_si256(p) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_load_si256(p: *const __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_movemask_epi8 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_movemask_epi8(a: __m256i) -> i32 {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_movemask_epi8(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_movemask_epi8(a: __m256i) -> i32 {
    loop {}
}

/// Native _mm256_shuffle_epi8 operation.
///
/// # Safety
/// The CPU must support avx2.
#[target_feature(enable = "avx2")]
pub fn _mm256_shuffle_epi8(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_shuffle_epi8(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_shuffle_epi8(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_slli_epi16 operation.
///
/// # Safety
/// The CPU must support avx2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "avx2")]
pub fn _mm256_slli_epi16<const IMM8: i32>(a: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_slli_epi16::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_slli_epi16<const IMM8: i32>(a: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_srli_epi16 operation.
///
/// # Safety
/// The CPU must support avx2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "avx2")]
pub fn _mm256_srli_epi16<const IMM8: i32>(a: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_srli_epi16::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_srli_epi16<const IMM8: i32>(a: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_slli_epi32 operation.
///
/// # Safety
/// The CPU must support avx2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "avx2")]
pub fn _mm256_slli_epi32<const IMM8: i32>(a: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_slli_epi32::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_slli_epi32<const IMM8: i32>(a: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_srli_epi32 operation.
///
/// # Safety
/// The CPU must support avx2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "avx2")]
pub fn _mm256_srli_epi32<const IMM8: i32>(a: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_srli_epi32::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_srli_epi32<const IMM8: i32>(a: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_slli_epi64 operation.
///
/// # Safety
/// The CPU must support avx2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "avx2")]
pub fn _mm256_slli_epi64<const IMM8: i32>(a: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_slli_epi64::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_slli_epi64<const IMM8: i32>(a: __m256i) -> __m256i {
    loop {}
}

/// Native _mm256_srli_epi64 operation.
///
/// # Safety
/// The CPU must support avx2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "avx2")]
pub fn _mm256_srli_epi64<const IMM8: i32>(a: __m256i) -> __m256i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm256_srli_epi64::<IMM8>(a) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_srli_epi64<const IMM8: i32>(a: __m256i) -> __m256i {
    loop {}
}

/// Native _mm_mulhi_epu16 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_mulhi_epu16(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_mulhi_epu16(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_mulhi_epu16(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_mul_epu32 operation.
///
/// # Safety
/// The CPU must support sse2.
#[target_feature(enable = "sse2")]
pub fn _mm_mul_epu32(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller supplies the CPU and memory requirements.
    unsafe { __x86_mm_mul_epu32(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_mul_epu32(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Constructs an all-zero vector.
#[target_feature(enable = "sse2")]
pub fn _mm_setzero_si128() -> __m128i {
    __m128i([0; 16])
}
/// Broadcasts one byte to every lane.
#[target_feature(enable = "sse2")]
pub fn _mm_set1_epi8(value: i8) -> __m128i {
    __m128i([value as u8; 16])
}
/// Constructs lanes from high to low.
#[target_feature(enable = "sse2")]
pub fn _mm_set_epi32(e3: i32, e2: i32, e1: i32, e0: i32) -> __m128i {
    // SAFETY: the lane array and vector have the same size and valid bit patterns.
    unsafe { crate::mem::transmute([e0, e1, e2, e3]) }
}
/// Constructs lanes from high to low.
#[target_feature(enable = "sse2")]
pub fn _mm_set_epi64x(e1: i64, e0: i64) -> __m128i {
    // SAFETY: the lane array and vector have the same size and valid bit patterns.
    unsafe { crate::mem::transmute([e0, e1]) }
}
/// Constructs an all-zero vector.
#[target_feature(enable = "avx")]
pub fn _mm256_setzero_si256() -> __m256i {
    __m256i([0; 32])
}
/// Broadcasts one byte to every lane.
#[target_feature(enable = "avx")]
pub fn _mm256_set1_epi8(value: i8) -> __m256i {
    __m256i([value as u8; 32])
}
/// Constructs lanes from high to low.
#[target_feature(enable = "avx")]
pub fn _mm256_set_epi32(
    e7: i32,
    e6: i32,
    e5: i32,
    e4: i32,
    e3: i32,
    e2: i32,
    e1: i32,
    e0: i32,
) -> __m256i {
    // SAFETY: the lane array and vector have the same size and valid bit patterns.
    unsafe { crate::mem::transmute([e0, e1, e2, e3, e4, e5, e6, e7]) }
}
/// Constructs lanes from high to low.
#[target_feature(enable = "avx")]
pub fn _mm256_set_epi64x(e3: i64, e2: i64, e1: i64, e0: i64) -> __m256i {
    // SAFETY: the lane array and vector have the same size and valid bit patterns.
    unsafe { crate::mem::transmute([e0, e1, e2, e3]) }
}
/// Constructs word lanes from low to high.
#[target_feature(enable = "sse2")]
pub fn _mm_setr_epi32(e0: i32, e1: i32, e2: i32, e3: i32) -> __m128i {
    _mm_set_epi32(e3, e2, e1, e0)
}
/// Returns the low signed doubleword.
#[target_feature(enable = "sse2")]
pub fn _mm_cvtsi128_si64(a: __m128i) -> i64 {
    // SAFETY: both representations contain exactly sixteen bytes.
    let lanes: [i64; 2] = unsafe { crate::mem::transmute(a) };
    lanes[0]
}
/// Stores sixteen bytes without an alignment requirement.
///
/// # Safety
/// `p` must address sixteen writable bytes, and the CPU must support SSE2.
#[target_feature(enable = "sse2")]
pub unsafe fn _mm_storeu_si128(p: *mut __m128i, a: __m128i) {
    // SAFETY: the caller supplies the writable byte range.
    unsafe { crate::ptr::write_unaligned(p, a) }
}

/// Four output registers from the CPUID instruction.
#[derive(Clone, Copy, Debug)]
pub struct CpuidResult {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}
/// Executes CPUID with a leaf and subleaf, available on every x86-64 processor.
pub fn __cpuid_count(leaf: u32, subleaf: u32) -> CpuidResult {
    // SAFETY: CPUID is available on x86-64, and register indices are in range.
    unsafe {
        CpuidResult {
            eax: __x86_cpuid(leaf, subleaf, 0),
            ebx: __x86_cpuid(leaf, subleaf, 1),
            ecx: __x86_cpuid(leaf, subleaf, 2),
            edx: __x86_cpuid(leaf, subleaf, 3),
        }
    }
}
/// Reads the enabled extended processor state components.
///
/// # Safety
/// CPUID must advertise OSXSAVE, and `index` must be zero.
pub unsafe fn _xgetbv(index: u32) -> u64 {
    assert!(index == 0, "only XCR0 is supported");
    // SAFETY: the caller establishes OSXSAVE support.
    unsafe { __x86_xgetbv(index) }
}
#[rustc_intrinsic]
unsafe fn __x86_cpuid(leaf: u32, subleaf: u32, register: u32) -> u32 {
    loop {}
}
#[rustc_intrinsic]
unsafe fn __x86_xgetbv(index: u32) -> u64 {
    loop {}
}
/// Native lane permutation.
///
/// # Safety
/// The CPU must support sse2.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse2")]
pub fn _mm_shuffle_epi32<const IMM8: i32>(a: __m128i) -> __m128i {
    // SAFETY: the caller establishes the required CPU feature.
    unsafe { __x86_mm_shuffle_epi32::<IMM8>(a) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_shuffle_epi32<const IMM8: i32>(a: __m128i) -> __m128i {
    loop {}
}
/// Native lane permutation.
///
/// # Safety
/// The CPU must support ssse3.
#[rustc_legacy_const_generics(2)]
#[target_feature(enable = "ssse3")]
pub fn _mm_alignr_epi8<const IMM8: i32>(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller establishes the required CPU feature.
    unsafe { __x86_mm_alignr_epi8::<IMM8>(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_alignr_epi8<const IMM8: i32>(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}
/// Native lane permutation.
///
/// # Safety
/// The CPU must support avx2.
#[rustc_legacy_const_generics(2)]
#[target_feature(enable = "avx2")]
pub fn _mm256_alignr_epi8<const IMM8: i32>(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the caller establishes the required CPU feature.
    unsafe { __x86_mm256_alignr_epi8::<IMM8>(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm256_alignr_epi8<const IMM8: i32>(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}
/// Native lane permutation.
///
/// # Safety
/// The CPU must support sse4.1.
#[rustc_legacy_const_generics(2)]
#[target_feature(enable = "sse4.1")]
pub fn _mm_blend_epi16<const IMM8: i32>(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the caller establishes the required CPU feature.
    unsafe { __x86_mm_blend_epi16::<IMM8>(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_blend_epi16<const IMM8: i32>(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}
/// Combines a high and a low 128-bit vector.
#[target_feature(enable = "avx")]
pub fn _mm256_set_m128i(hi: __m128i, lo: __m128i) -> __m256i {
    let mut bytes = [0u8; 32];
    for i in 0..16 {
        bytes[i] = lo.0[i];
        bytes[i + 16] = hi.0[i];
    }
    __m256i(bytes)
}
/// Extracts one 128-bit half.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "avx2")]
pub fn _mm256_extracti128_si256<const IMM8: i32>(a: __m256i) -> __m128i {
    let mut bytes = [0u8; 16];
    for i in 0..16 {
        bytes[i] = a.0[i + IMM8 as usize * 16];
    }
    __m128i(bytes)
}
/// Replaces one 128-bit half.
#[rustc_legacy_const_generics(2)]
#[target_feature(enable = "avx")]
pub fn _mm256_insertf128_si256<const IMM8: i32>(a: __m256i, b: __m128i) -> __m256i {
    let mut bytes = a.0;
    for i in 0..16 {
        bytes[i + IMM8 as usize * 16] = b.0[i];
    }
    __m256i(bytes)
}

impl crate::fmt::Debug for __m128i {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        // SAFETY: the array and vector have equal sizes and valid integer bits.
        let lanes: [i64; 2] = unsafe { crate::mem::transmute(*self) };
        let mut tuple = f.debug_tuple("__m128i");
        for lane in &lanes {
            tuple.field(lane);
        }
        tuple.finish()
    }
}

impl crate::fmt::Debug for __m256i {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        // SAFETY: the array and vector have equal sizes and valid integer bits.
        let lanes: [i64; 4] = unsafe { crate::mem::transmute(*self) };
        let mut tuple = f.debug_tuple("__m256i");
        for lane in &lanes {
            tuple.field(lane);
        }
        tuple.finish()
    }
}

/// Stores sixteen bytes at an aligned address.
///
/// # Safety
/// The CPU must support SSE2. `p` must address sixteen writable bytes aligned to sixteen bytes.
#[target_feature(enable = "sse2")]
pub unsafe fn _mm_store_si128(p: *mut __m128i, a: __m128i) {
    // SAFETY: the caller supplies a writable, aligned vector address.
    unsafe { crate::ptr::write(p, a) }
}
/// Packed integer _mm_add_epi8 operation.
#[target_feature(enable = "sse2")]
pub fn _mm_add_epi8(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the annotated function establishes SSE2 support.
    unsafe { __x86_mm_add_epi8(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_add_epi8(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}
/// Packed integer _mm_cmpeq_epi16 operation.
#[target_feature(enable = "sse2")]
pub fn _mm_cmpeq_epi16(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the annotated function establishes SSE2 support.
    unsafe { __x86_mm_cmpeq_epi16(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_cmpeq_epi16(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}
/// Packed integer _mm_cmpeq_epi32 operation.
#[target_feature(enable = "sse2")]
pub fn _mm_cmpeq_epi32(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the annotated function establishes SSE2 support.
    unsafe { __x86_mm_cmpeq_epi32(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_cmpeq_epi32(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}
/// Packed integer _mm_cmpgt_epi16 operation.
#[target_feature(enable = "sse2")]
pub fn _mm_cmpgt_epi16(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the annotated function establishes SSE2 support.
    unsafe { __x86_mm_cmpgt_epi16(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_cmpgt_epi16(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}
/// Packed integer _mm_cmpgt_epi32 operation.
#[target_feature(enable = "sse2")]
pub fn _mm_cmpgt_epi32(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the annotated function establishes SSE2 support.
    unsafe { __x86_mm_cmpgt_epi32(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_cmpgt_epi32(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}
/// Packed integer _mm_packs_epi16 operation.
#[target_feature(enable = "sse2")]
pub fn _mm_packs_epi16(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the annotated function establishes SSE2 support.
    unsafe { __x86_mm_packs_epi16(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_packs_epi16(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}
/// Packed integer _mm_packs_epi32 operation.
#[target_feature(enable = "sse2")]
pub fn _mm_packs_epi32(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the annotated function establishes SSE2 support.
    unsafe { __x86_mm_packs_epi32(a, b) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_packs_epi32(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}
/// Extracts or shifts packed integer lanes.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse2")]
pub fn _mm_srli_si128<const IMM8: i32>(a: __m128i) -> __m128i {
    // SAFETY: the annotated function establishes the CPU feature.
    unsafe { __x86_mm_srli_si128::<IMM8>(a) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_srli_si128<const IMM8: i32>(a: __m128i) -> __m128i {
    loop {}
}
/// Extracts or shifts packed integer lanes.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse4.1")]
pub fn _mm_extract_epi32<const IMM8: i32>(a: __m128i) -> i32 {
    // SAFETY: the annotated function establishes the CPU feature.
    unsafe { __x86_mm_extract_epi32::<IMM8>(a) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_extract_epi32<const IMM8: i32>(a: __m128i) -> i32 {
    loop {}
}
/// The non-temporal prefetch strategy.
pub const _MM_HINT_NTA: i32 = 0;
/// Prefetch into all cache levels.
pub const _MM_HINT_T0: i32 = 3;
/// Prefetch into the second and later cache levels.
pub const _MM_HINT_T1: i32 = 2;
/// Prefetch into the last-level cache.
pub const _MM_HINT_T2: i32 = 1;
/// Issues a non-faulting cache prefetch hint.
///
/// # Safety
/// The processor must support SSE.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "sse")]
pub unsafe fn _mm_prefetch<const STRATEGY: i32>(p: *const i8) {
    // SAFETY: the caller establishes SSE support; prefetch does not dereference p.
    unsafe { __x86_mm_prefetch::<STRATEGY>(p) }
}
#[rustc_intrinsic]
unsafe fn __x86_mm_prefetch<const STRATEGY: i32>(p: *const i8) {
    loop {}
}
/// Places a word in the low lane and clears the upper lanes.
#[target_feature(enable = "sse2")]
pub fn _mm_cvtsi32_si128(value: i32) -> __m128i {
    _mm_set_epi32(0, 0, 0, value)
}
/// Broadcasts a halfword into every lane.
#[target_feature(enable = "sse2")]
pub fn _mm_set1_epi16(value: i16) -> __m128i {
    // SAFETY: both representations contain sixteen initialized bytes.
    unsafe { crate::mem::transmute([value; 8]) }
}
/// Broadcasts a word into every lane.
#[target_feature(enable = "sse2")]
pub fn _mm_set1_epi32(value: i32) -> __m128i {
    _mm_set_epi32(value, value, value, value)
}
/// Constructs byte lanes from low to high.
#[target_feature(enable = "sse2")]
pub fn _mm_setr_epi8(
    e0: i8,
    e1: i8,
    e2: i8,
    e3: i8,
    e4: i8,
    e5: i8,
    e6: i8,
    e7: i8,
    e8: i8,
    e9: i8,
    e10: i8,
    e11: i8,
    e12: i8,
    e13: i8,
    e14: i8,
    e15: i8,
) -> __m128i {
    __m128i([
        e0 as u8, e1 as u8, e2 as u8, e3 as u8, e4 as u8, e5 as u8, e6 as u8, e7 as u8, e8 as u8,
        e9 as u8, e10 as u8, e11 as u8, e12 as u8, e13 as u8, e14 as u8, e15 as u8,
    ])
}
/// Widens a vector; upper lanes are unspecified and currently zeroed.
#[target_feature(enable = "avx")]
pub fn _mm256_castsi128_si256(a: __m128i) -> __m256i {
    _mm256_set_m128i(__m128i([0; 16]), a)
}
/// Extracts the low 128 bits without changing their representation.
#[target_feature(enable = "avx")]
pub fn _mm256_castsi256_si128(a: __m256i) -> __m128i {
    let mut bytes = [0u8; 16];
    for i in 0..16 {
        bytes[i] = a.0[i];
    }
    __m128i(bytes)
}

/// 64 packed bytes with native AVX-512 vector alignment.
#[allow(non_camel_case_types)]
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct __m512i(pub(crate) [u8; 64]);

impl crate::fmt::Debug for __m512i {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        // SAFETY: every vector bit pattern is a valid array of eight i64 lanes.
        let lanes: [i64; 8] = unsafe { crate::mem::transmute(*self) };
        let mut tuple = f.debug_tuple("__m512i");
        for lane in &lanes {
            tuple.field(lane);
        }
        tuple.finish()
    }
}

/// Constructs an all-zero vector.
#[target_feature(enable = "avx512f")]
pub fn _mm512_setzero_si512() -> __m512i {
    __m512i([0; 64])
}

/// Constructs eight lanes from high to low.
#[target_feature(enable = "avx512f")]
pub fn _mm512_set_epi64(
    e7: i64,
    e6: i64,
    e5: i64,
    e4: i64,
    e3: i64,
    e2: i64,
    e1: i64,
    e0: i64,
) -> __m512i {
    // SAFETY: both representations contain the same 64 initialized bytes.
    unsafe { crate::mem::transmute([e0, e1, e2, e3, e4, e5, e6, e7]) }
}

/// Widens the low 128 bits; the unspecified upper bits are currently zeroed.
#[target_feature(enable = "avx512f")]
pub fn _mm512_castsi128_si512(a: __m128i) -> __m512i {
    let mut bytes = [0u8; 64];
    for i in 0..16 {
        bytes[i] = a.0[i];
    }
    __m512i(bytes)
}

/// Extracts the low 128 bits without changing their representation.
#[target_feature(enable = "avx512f")]
pub fn _mm512_castsi512_si128(a: __m512i) -> __m128i {
    _mm512_extracti32x4_epi32::<0>(a)
}

/// Extracts one of four 128-bit quarters.
#[rustc_legacy_const_generics(1)]
#[target_feature(enable = "avx512f")]
pub fn _mm512_extracti32x4_epi32<const IMM8: i32>(a: __m512i) -> __m128i {
    let mut bytes = [0u8; 16];
    for i in 0..16 {
        bytes[i] = a.0[IMM8 as usize * 16 + i];
    }
    __m128i(bytes)
}

/// Stores 64 bytes without an alignment requirement.
///
/// # Safety
/// The CPU must support AVX-512F and `p` must address 64 writable bytes.
#[target_feature(enable = "avx512f")]
pub unsafe fn _mm512_storeu_si512(p: *mut __m512i, a: __m512i) {
    // SAFETY: the caller supplies 64 writable bytes; byte stores need no alignment.
    unsafe { crate::ptr::copy_nonoverlapping(a.0.as_ptr(), p as *mut u8, 64) }
}

/// Queries a CPUID leaf with subleaf zero.
pub fn __cpuid(leaf: u32) -> CpuidResult {
    __cpuid_count(leaf, 0)
}

/// XGETBV selector for the enabled extended-state mask in XCR0.
pub const _XCR_XFEATURE_ENABLED_MASK: u32 = 0;

/// Native _mm512_xor_si512 operation.
#[target_feature(enable = "avx512f")]
pub fn _mm512_xor_si512(a: __m512i, b: __m512i) -> __m512i {
    // SAFETY: the function establishes CPU support; the caller supplies valid memory where required.
    unsafe { __x86_mm512_xor_si512(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm512_xor_si512(a: __m512i, b: __m512i) -> __m512i {
    loop {}
}

/// Native _mm512_loadu_si512 operation.
///
/// # Safety
/// The CPU must support avx512f; `p` must address 64 readable bytes, without an alignment requirement.
#[target_feature(enable = "avx512f")]
pub unsafe fn _mm512_loadu_si512(p: *const __m512i) -> __m512i {
    // SAFETY: the function establishes CPU support; the caller supplies valid memory where required.
    unsafe { __x86_mm512_loadu_si512(p) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm512_loadu_si512(p: *const __m512i) -> __m512i {
    loop {}
}

/// Native _mm_clmulepi64_si128 operation.
#[rustc_legacy_const_generics(2)]
#[target_feature(enable = "pclmulqdq")]
pub fn _mm_clmulepi64_si128<const IMM8: i32>(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the function establishes CPU support; the caller supplies valid memory where required.
    unsafe { __x86_mm_clmulepi64_si128::<IMM8>(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_clmulepi64_si128<const IMM8: i32>(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm256_clmulepi64_epi128 operation.
#[rustc_legacy_const_generics(2)]
#[target_feature(enable = "vpclmulqdq,avx")]
pub fn _mm256_clmulepi64_epi128<const IMM8: i32>(a: __m256i, b: __m256i) -> __m256i {
    // SAFETY: the function establishes CPU support; the caller supplies valid memory where required.
    unsafe { __x86_mm256_clmulepi64_epi128::<IMM8>(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm256_clmulepi64_epi128<const IMM8: i32>(a: __m256i, b: __m256i) -> __m256i {
    loop {}
}

/// Native _mm512_clmulepi64_epi128 operation.
#[rustc_legacy_const_generics(2)]
#[target_feature(enable = "vpclmulqdq,avx512f")]
pub fn _mm512_clmulepi64_epi128<const IMM8: i32>(a: __m512i, b: __m512i) -> __m512i {
    // SAFETY: the function establishes CPU support; the caller supplies valid memory where required.
    unsafe { __x86_mm512_clmulepi64_epi128::<IMM8>(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm512_clmulepi64_epi128<const IMM8: i32>(a: __m512i, b: __m512i) -> __m512i {
    loop {}
}

/// Native _mm_sha256msg1_epu32 operation.
#[target_feature(enable = "sha")]
pub fn _mm_sha256msg1_epu32(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the function establishes CPU support; the caller supplies valid memory where required.
    unsafe { __x86_mm_sha256msg1_epu32(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_sha256msg1_epu32(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_sha256msg2_epu32 operation.
#[target_feature(enable = "sha")]
pub fn _mm_sha256msg2_epu32(a: __m128i, b: __m128i) -> __m128i {
    // SAFETY: the function establishes CPU support; the caller supplies valid memory where required.
    unsafe { __x86_mm_sha256msg2_epu32(a, b) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_sha256msg2_epu32(a: __m128i, b: __m128i) -> __m128i {
    loop {}
}

/// Native _mm_sha256rnds2_epu32 operation.
#[target_feature(enable = "sha")]
pub fn _mm_sha256rnds2_epu32(a: __m128i, b: __m128i, k: __m128i) -> __m128i {
    // SAFETY: the function establishes CPU support; the caller supplies valid memory where required.
    unsafe { __x86_mm_sha256rnds2_epu32(a, b, k) }
}

#[rustc_intrinsic]
unsafe fn __x86_mm_sha256rnds2_epu32(a: __m128i, b: __m128i, k: __m128i) -> __m128i {
    loop {}
}
