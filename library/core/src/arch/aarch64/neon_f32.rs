//! AArch64 floating-point vectors and their integer masks.

use super::{float32x2_t, float32x4_t, int32x4_t, uint32x4_t, uint64x2_t, vextq_u32};
use crate::arch::asm;

macro_rules! binary {
    ($doc:literal, $name:ident, $input:ty, $output:ty, $instruction:literal) => {
        #[doc = $doc]
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name(a: $input, b: $input) -> $output {
            let result;
            // SAFETY: the instruction reads two initialized SIMD registers
            // and writes only the declared result register.
            unsafe {
                asm!($instruction, lateout(vreg) result, in(vreg) a, in(vreg) b,
                     options(nomem, nostack, preserves_flags));
            }
            result
        }
    };
}

macro_rules! unary {
    ($doc:literal, $name:ident, $input:ty, $output:ty, $instruction:literal) => {
        #[doc = $doc]
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name(value: $input) -> $output {
            let result;
            // SAFETY: the instruction reads one initialized SIMD register
            // and writes only the declared result register.
            unsafe {
                asm!($instruction, lateout(vreg) result, in(vreg) value,
                     options(nomem, nostack, preserves_flags));
            }
            result
        }
    };
}

binary!(
    "Adds corresponding lanes.",
    vaddq_f32,
    float32x4_t,
    float32x4_t,
    "fadd {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Subtracts corresponding lanes.",
    vsub_f32,
    float32x2_t,
    float32x2_t,
    "fsub {0:v}.2s, {1:v}.2s, {2:v}.2s"
);
binary!(
    "Subtracts corresponding lanes.",
    vsubq_f32,
    float32x4_t,
    float32x4_t,
    "fsub {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Multiplies corresponding lanes.",
    vmulq_f32,
    float32x4_t,
    float32x4_t,
    "fmul {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Divides corresponding lanes.",
    vdivq_f32,
    float32x4_t,
    float32x4_t,
    "fdiv {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Returns each pair's maximum, propagating NaNs.",
    vmaxq_f32,
    float32x4_t,
    float32x4_t,
    "fmax {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Returns each pair's minimum, propagating NaNs.",
    vminq_f32,
    float32x4_t,
    float32x4_t,
    "fmin {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Tests whether corresponding lanes are equal.",
    vceqq_f32,
    float32x4_t,
    uint32x4_t,
    "fcmeq {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Tests whether each left lane is greater than or equal to the right lane.",
    vcgeq_f32,
    float32x4_t,
    uint32x4_t,
    "fcmge {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Tests whether each left lane is greater than the right lane.",
    vcgtq_f32,
    float32x4_t,
    uint32x4_t,
    "fcmgt {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Tests whether each left lane is less than or equal to the right lane.",
    vcleq_f32,
    float32x4_t,
    uint32x4_t,
    "fcmge {0:v}.4s, {2:v}.4s, {1:v}.4s"
);
binary!(
    "Tests whether each left lane is less than the right lane.",
    vcltq_f32,
    float32x4_t,
    uint32x4_t,
    "fcmgt {0:v}.4s, {2:v}.4s, {1:v}.4s"
);
binary!(
    "Tests whether each signed left lane is less than the right lane.",
    vcltq_s32,
    int32x4_t,
    uint32x4_t,
    "cmgt {0:v}.4s, {2:v}.4s, {1:v}.4s"
);

unary!(
    "Takes the absolute value of each lane.",
    vabsq_f32,
    float32x4_t,
    float32x4_t,
    "fabs {0:v}.4s, {1:v}.4s"
);
unary!(
    "Negates each lane.",
    vnegq_f32,
    float32x4_t,
    float32x4_t,
    "fneg {0:v}.4s, {1:v}.4s"
);
unary!(
    "Rounds to the nearest integer, with ties away from zero.",
    vrndaq_f32,
    float32x4_t,
    float32x4_t,
    "frinta {0:v}.4s, {1:v}.4s"
);
unary!(
    "Rounds toward negative infinity.",
    vrndmq_f32,
    float32x4_t,
    float32x4_t,
    "frintm {0:v}.4s, {1:v}.4s"
);
unary!(
    "Rounds to the nearest integer, with ties to even.",
    vrndnq_f32,
    float32x4_t,
    float32x4_t,
    "frintn {0:v}.4s, {1:v}.4s"
);
unary!(
    "Rounds toward positive infinity.",
    vrndpq_f32,
    float32x4_t,
    float32x4_t,
    "frintp {0:v}.4s, {1:v}.4s"
);
unary!(
    "Rounds toward zero.",
    vrndq_f32,
    float32x4_t,
    float32x4_t,
    "frintz {0:v}.4s, {1:v}.4s"
);
unary!(
    "Finds the largest number across the lanes.",
    vmaxnmvq_f32,
    float32x4_t,
    f32,
    "fmaxnmv {0:s}, {1:v}.4s"
);
unary!(
    "Finds the smallest number across the lanes.",
    vminnmvq_f32,
    float32x4_t,
    f32,
    "fminnmv {0:s}, {1:v}.4s"
);
unary!(
    "Complements every bit of the vector.",
    vmvnq_u32,
    uint32x4_t,
    uint32x4_t,
    "not {0:v}.16b, {1:v}.16b"
);

binary!(
    "Interleaves the even lanes of both vectors.",
    vtrn1q_f32,
    float32x4_t,
    float32x4_t,
    "trn1 {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Interleaves the odd lanes of both vectors.",
    vtrn2q_f32,
    float32x4_t,
    float32x4_t,
    "trn2 {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Packs the even lanes of each vector in order.",
    vuzp1q_f32,
    float32x4_t,
    float32x4_t,
    "uzp1 {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Packs the odd lanes of each vector in order.",
    vuzp2q_f32,
    float32x4_t,
    float32x4_t,
    "uzp2 {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Interleaves the lower halves of both vectors.",
    vzip1q_f32,
    float32x4_t,
    float32x4_t,
    "zip1 {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Interleaves the upper halves of both vectors.",
    vzip2q_f32,
    float32x4_t,
    float32x4_t,
    "zip2 {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
binary!(
    "Interleaves the upper halves of both vectors.",
    vzip2q_u64,
    uint64x2_t,
    uint64x2_t,
    "zip2 {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
unary!(
    "Reverses the two lanes within each 64-bit half.",
    vrev64q_f32,
    float32x4_t,
    float32x4_t,
    "rev64 {0:v}.4s, {1:v}.4s"
);

reinterpret128!(vreinterpretq_f32_u32, uint32x4_t, float32x4_t);
reinterpret128!(vreinterpretq_f32_u64, uint64x2_t, float32x4_t);
reinterpret128!(vreinterpretq_s32_f32, float32x4_t, int32x4_t);
reinterpret128!(vreinterpretq_u32_f32, float32x4_t, uint32x4_t);
reinterpret128!(vreinterpretq_u64_f32, float32x4_t, uint64x2_t);

/// Adds the lanes using two stages of pairwise addition.
#[inline]
#[target_feature(enable = "neon")]
pub fn vaddvq_f32(value: float32x4_t) -> f32 {
    let result;
    // SAFETY: both instructions use only the declared inputs and outputs.
    unsafe {
        asm!("faddp {0:v}.4s, {0:v}.4s, {0:v}.4s",
             "faddp {1:s}, {0:v}.2s",
             inout(vreg) value => _, lateout(vreg) result,
             options(nomem, nostack, preserves_flags));
    }
    result
}

/// Selects each result bit from `a` if its mask bit is one, or `b` otherwise.
#[inline]
#[target_feature(enable = "neon")]
pub fn vbslq_f32(mask: uint32x4_t, a: float32x4_t, b: float32x4_t) -> float32x4_t {
    let result: uint32x4_t;
    // SAFETY: BSL reads three initialized SIMD registers and overwrites its
    // tied mask register. The result is reinterpreted without conversion.
    unsafe {
        asm!("bsl {0:v}.16b, {1:v}.16b, {2:v}.16b",
             inout(vreg) mask => result, in(vreg) a, in(vreg) b,
             options(nomem, nostack, preserves_flags));
    }
    vreinterpretq_f32_u32(result)
}

macro_rules! fused {
    ($doc:literal, $name:ident, $instruction:literal) => {
        #[doc = $doc]
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name(mut accumulator: float32x4_t, a: float32x4_t, b: float32x4_t) -> float32x4_t {
            // SAFETY: the instruction reads three initialized SIMD registers
            // and overwrites only its tied accumulator.
            unsafe {
                asm!($instruction, inout(vreg) accumulator, in(vreg) a, in(vreg) b,
                     options(nomem, nostack, preserves_flags));
            }
            accumulator
        }
    };
}

fused!(
    "Adds a product to each lane with one rounding.",
    vfmaq_f32,
    "fmla {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
fused!(
    "Subtracts a product from each lane with one rounding.",
    vfmsq_f32,
    "fmls {0:v}.4s, {1:v}.4s, {2:v}.4s"
);

/// Adds a separately rounded product to each lane.
#[inline]
#[target_feature(enable = "neon")]
pub fn vmlaq_f32(a: float32x4_t, b: float32x4_t, c: float32x4_t) -> float32x4_t {
    vaddq_f32(a, vmulq_f32(b, c))
}

/// Subtracts a separately rounded product from each lane.
#[inline]
#[target_feature(enable = "neon")]
pub fn vmlsq_f32(a: float32x4_t, b: float32x4_t, c: float32x4_t) -> float32x4_t {
    vsubq_f32(a, vmulq_f32(b, c))
}

/// Adds the product of each lane of `b` and one lane of `c` with one rounding.
#[rustc_legacy_const_generics(3)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vfmaq_lane_f32<const N: i32>(a: float32x4_t, b: float32x4_t, c: float32x2_t) -> float32x4_t {
    vfmaq_f32(a, b, vdupq_lane_f32::<N>(c))
}

/// Adds the separately rounded product of each lane of `b` and one lane of `c`.
#[rustc_legacy_const_generics(3)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vmlaq_lane_f32<const N: i32>(a: float32x4_t, b: float32x4_t, c: float32x2_t) -> float32x4_t {
    vmlaq_f32(a, b, vdupq_lane_f32::<N>(c))
}

/// Broadcasts a floating-point value to all lanes.
#[inline]
#[target_feature(enable = "neon")]
pub fn vdupq_n_f32(value: f32) -> float32x4_t {
    float32x4_t([value; 4])
}

/// Broadcasts a signed word to all lanes.
#[inline]
#[target_feature(enable = "neon")]
pub fn vdupq_n_s32(value: i32) -> int32x4_t {
    int32x4_t([value; 4])
}

/// Broadcasts one lane of a two-lane vector.
#[rustc_legacy_const_generics(1)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vdupq_lane_f32<const N: i32>(value: float32x2_t) -> float32x4_t {
    vdupq_n_f32(vget_lane_f32::<N>(value))
}

/// Broadcasts one lane of a four-lane vector.
#[rustc_legacy_const_generics(1)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vdupq_laneq_f32<const N: i32>(value: float32x4_t) -> float32x4_t {
    vdupq_n_f32(vgetq_lane_f32::<N>(value))
}

/// Multiplies both lanes of `a` by one lane of `b`.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vmul_lane_f32<const N: i32>(a: float32x2_t, b: float32x2_t) -> float32x2_t {
    let lane = vget_lane_f32::<N>(b);
    float32x2_t([a.0[0] * lane, a.0[1] * lane])
}

/// Multiplies each lane of `a` by a scalar.
#[inline]
#[target_feature(enable = "neon")]
pub fn vmulq_n_f32(a: float32x4_t, b: f32) -> float32x4_t {
    vmulq_f32(a, vdupq_n_f32(b))
}

/// Multiplies a scalar by one vector lane.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vmuls_laneq_f32<const N: i32>(a: f32, b: float32x4_t) -> f32 {
    a * vgetq_lane_f32::<N>(b)
}

/// Returns the low two lanes.
#[inline]
#[target_feature(enable = "neon")]
pub fn vget_low_f32(value: float32x4_t) -> float32x2_t {
    float32x2_t([value.0[0], value.0[1]])
}

/// Returns the high two lanes.
#[inline]
#[target_feature(enable = "neon")]
pub fn vget_high_f32(value: float32x4_t) -> float32x2_t {
    float32x2_t([value.0[2], value.0[3]])
}

/// Extracts one lane of a two-lane vector.
#[rustc_legacy_const_generics(1)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vget_lane_f32<const N: i32>(value: float32x2_t) -> f32 {
    value.0[N as usize]
}

/// Extracts one lane of a four-lane vector.
#[rustc_legacy_const_generics(1)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vgetq_lane_f32<const N: i32>(value: float32x4_t) -> f32 {
    value.0[N as usize]
}

/// Replaces one floating-point lane.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vsetq_lane_f32<const N: i32>(lane: f32, mut value: float32x4_t) -> float32x4_t {
    value.0[N as usize] = lane;
    value
}

/// Replaces one unsigned word lane.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vsetq_lane_u32<const N: i32>(lane: u32, mut value: uint32x4_t) -> uint32x4_t {
    value.0[N as usize] = lane;
    value
}

/// Extracts four lanes starting at N in the concatenated vectors.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vextq_f32<const N: i32>(a: float32x4_t, b: float32x4_t) -> float32x4_t {
    vreinterpretq_f32_u32(vextq_u32::<N>(
        vreinterpretq_u32_f32(a),
        vreinterpretq_u32_f32(b),
    ))
}

/// Broadcasts one loaded floating-point value to all lanes.
///
/// # Safety
/// The pointer must address one initialized readable `f32`.
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vld1q_dup_f32(pointer: *const f32) -> float32x4_t {
    // SAFETY: the caller supplies one readable lane. Alignment is unrestricted.
    vdupq_n_f32(unsafe { crate::ptr::read_unaligned(pointer) })
}

/// Loads four consecutive floating-point lanes.
///
/// # Safety
/// The pointer must address four initialized readable `f32` lanes.
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vld1q_f32(pointer: *const f32) -> float32x4_t {
    // SAFETY: the vector and four lanes have identical storage. Alignment is unrestricted.
    unsafe { crate::ptr::read_unaligned(pointer.cast::<float32x4_t>()) }
}

/// Stores the four floating-point lanes in order.
///
/// # Safety
/// The pointer must address four writable `f32` lanes.
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vst1q_f32(pointer: *mut f32, value: float32x4_t) {
    // SAFETY: the vector and four lanes have identical storage. Alignment is unrestricted.
    unsafe { crate::ptr::write_unaligned(pointer.cast::<float32x4_t>(), value) }
}

pub use self::vdupq_n_f32 as vmovq_n_f32;

#[rustc_legacy_const_generics(3)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vfmaq_laneq_f32<const LANE: i32>(
    a: float32x4_t,
    b: float32x4_t,
    c: float32x4_t,
) -> float32x4_t {
    vfmaq_f32(a, b, vdupq_laneq_f32::<LANE>(c))
}

/// # Safety
/// The pointer must address one initialized readable lane.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vld1q_lane_f32<const LANE: i32>(
    pointer: *const f32,
    value: float32x4_t,
) -> float32x4_t {
    // SAFETY: the caller supplies a readable lane. Alignment is unrestricted.
    vsetq_lane_f32::<LANE>(unsafe { crate::ptr::read_unaligned(pointer) }, value)
}

/// # Safety
/// The pointer must address one writable lane.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vst1q_lane_f32<const LANE: i32>(pointer: *mut f32, value: float32x4_t) {
    // SAFETY: the caller supplies a writable lane. Alignment is unrestricted.
    unsafe { crate::ptr::write_unaligned(pointer, vgetq_lane_f32::<LANE>(value)) }
}

neon_unary!(
    vcvtnq_s32_f32,
    float32x4_t,
    int32x4_t,
    "fcvtns {0:v}.4s, {1:v}.4s"
);
neon_unary!(
    vcvtq_f32_s32,
    int32x4_t,
    float32x4_t,
    "scvtf {0:v}.4s, {1:v}.4s"
);
neon_unary!(
    vcvtq_s32_f32,
    float32x4_t,
    int32x4_t,
    "fcvtzs {0:v}.4s, {1:v}.4s"
);
neon_binary!(
    vmaxnmq_f32,
    float32x4_t,
    float32x4_t,
    float32x4_t,
    "fmaxnm {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vminnmq_f32,
    float32x4_t,
    float32x4_t,
    float32x4_t,
    "fminnm {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_unary!(
    vsqrtq_f32,
    float32x4_t,
    float32x4_t,
    "fsqrt {0:v}.4s, {1:v}.4s"
);
