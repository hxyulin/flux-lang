//! AArch64 double-precision arithmetic and lane access.
use super::{float64x2_t, int64x2_t, uint64x2_t};
use crate::arch::asm;

#[inline]
#[target_feature(enable = "neon")]
pub fn vdupq_n_f64(value: f64) -> float64x2_t {
    float64x2_t([value; 2])
}

pub use self::vdupq_n_f64 as vmovq_n_f64;

#[inline]
#[target_feature(enable = "neon")]
pub fn vmulq_n_f64(value: float64x2_t, multiplier: f64) -> float64x2_t {
    float64x2_t([value.0[0] * multiplier, value.0[1] * multiplier])
}

#[inline]
#[target_feature(enable = "neon")]
pub fn vfmaq_f64(mut accumulator: float64x2_t, a: float64x2_t, b: float64x2_t) -> float64x2_t {
    // SAFETY: the instruction reads the initialized lanes and updates the tied accumulator.
    unsafe {
        asm!("fmla {0:v}.2d, {1:v}.2d, {2:v}.2d",
             inout(vreg) accumulator, in(vreg) a, in(vreg) b,
             options(nomem, nostack, preserves_flags));
    }
    accumulator
}

#[rustc_legacy_const_generics(3)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vfmaq_laneq_f64<const LANE: i32>(
    a: float64x2_t,
    b: float64x2_t,
    c: float64x2_t,
) -> float64x2_t {
    vfmaq_f64(a, b, vdupq_n_f64(c.0[LANE as usize]))
}

/// # Safety
/// The pointer must address one initialized readable lane.
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vld1q_dup_f64(pointer: *const f64) -> float64x2_t {
    // SAFETY: the caller supplies a readable lane. Alignment is unrestricted.
    vdupq_n_f64(unsafe { crate::ptr::read_unaligned(pointer) })
}

/// # Safety
/// The pointer must address two initialized readable lanes.
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vld1q_f64(pointer: *const f64) -> float64x2_t {
    // SAFETY: the two lanes have the same storage as the vector.
    unsafe { crate::ptr::read_unaligned(pointer.cast::<float64x2_t>()) }
}

/// # Safety
/// The pointer must address two writable lanes.
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vst1q_f64(pointer: *mut f64, value: float64x2_t) {
    // SAFETY: the vector has the same storage as the two writable lanes.
    unsafe { crate::ptr::write_unaligned(pointer.cast::<float64x2_t>(), value) }
}

/// # Safety
/// The pointer must address one initialized readable lane.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vld1q_lane_f64<const LANE: i32>(
    pointer: *const f64,
    mut value: float64x2_t,
) -> float64x2_t {
    // SAFETY: the caller supplies a readable lane. Alignment is unrestricted.
    value.0[LANE as usize] = unsafe { crate::ptr::read_unaligned(pointer) };
    value
}

/// # Safety
/// The pointer must address one writable lane.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vst1q_lane_f64<const LANE: i32>(pointer: *mut f64, value: float64x2_t) {
    // SAFETY: the caller supplies a writable lane. Alignment is unrestricted.
    unsafe { crate::ptr::write_unaligned(pointer, value.0[LANE as usize]) }
}

neon_unary!(
    vabsq_f64,
    float64x2_t,
    float64x2_t,
    "fabs {0:v}.2d, {1:v}.2d"
);
neon_binary!(
    vaddq_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "fadd {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vceqq_f64,
    float64x2_t,
    float64x2_t,
    uint64x2_t,
    "fcmeq {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vcgeq_f64,
    float64x2_t,
    float64x2_t,
    uint64x2_t,
    "fcmge {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vcgtq_f64,
    float64x2_t,
    float64x2_t,
    uint64x2_t,
    "fcmgt {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vcleq_f64,
    float64x2_t,
    float64x2_t,
    uint64x2_t,
    "fcmge {0:v}.2d, {2:v}.2d, {1:v}.2d"
);
neon_binary!(
    vcltq_f64,
    float64x2_t,
    float64x2_t,
    uint64x2_t,
    "fcmgt {0:v}.2d, {2:v}.2d, {1:v}.2d"
);
neon_unary!(
    vcvtnq_s64_f64,
    float64x2_t,
    int64x2_t,
    "fcvtns {0:v}.2d, {1:v}.2d"
);
neon_unary!(
    vcvtq_f64_s64,
    int64x2_t,
    float64x2_t,
    "scvtf {0:v}.2d, {1:v}.2d"
);
neon_unary!(
    vcvtq_s64_f64,
    float64x2_t,
    int64x2_t,
    "fcvtzs {0:v}.2d, {1:v}.2d"
);
neon_binary!(
    vdivq_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "fdiv {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vmaxnmq_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "fmaxnm {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vmaxq_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "fmax {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vminnmq_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "fminnm {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vminq_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "fmin {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vmulq_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "fmul {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_unary!(
    vnegq_f64,
    float64x2_t,
    float64x2_t,
    "fneg {0:v}.2d, {1:v}.2d"
);
neon_unary!(
    vrndmq_f64,
    float64x2_t,
    float64x2_t,
    "frintm {0:v}.2d, {1:v}.2d"
);
neon_unary!(
    vrndpq_f64,
    float64x2_t,
    float64x2_t,
    "frintp {0:v}.2d, {1:v}.2d"
);
neon_unary!(
    vsqrtq_f64,
    float64x2_t,
    float64x2_t,
    "fsqrt {0:v}.2d, {1:v}.2d"
);
neon_binary!(
    vsubq_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "fsub {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vzip1q_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "zip1 {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vzip2q_f64,
    float64x2_t,
    float64x2_t,
    float64x2_t,
    "zip2 {0:v}.2d, {1:v}.2d, {2:v}.2d"
);

/// Subtracts a product from each lane with one rounding.
#[inline]
#[target_feature(enable = "neon")]
pub fn vfmsq_f64(mut accumulator: float64x2_t, a: float64x2_t, b: float64x2_t) -> float64x2_t {
    // SAFETY: the instruction reads the initialized lanes and updates the tied accumulator.
    unsafe {
        asm!("fmls {0:v}.2d, {1:v}.2d, {2:v}.2d",
             inout(vreg) accumulator, in(vreg) a, in(vreg) b,
             options(nomem, nostack));
    }
    accumulator
}
