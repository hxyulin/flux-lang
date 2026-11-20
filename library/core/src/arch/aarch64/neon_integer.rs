//! AArch64 integer arithmetic, comparisons and reductions.
use super::*;

neon_unary!(vabsq_s16, int16x8_t, int16x8_t, "abs {0:v}.8h, {1:v}.8h");
neon_unary!(vabsq_s32, int32x4_t, int32x4_t, "abs {0:v}.4s, {1:v}.4s");
neon_unary!(vabsq_s64, int64x2_t, int64x2_t, "abs {0:v}.2d, {1:v}.2d");
neon_unary!(vabsq_s8, int8x16_t, int8x16_t, "abs {0:v}.16b, {1:v}.16b");
neon_binary!(
    vceqq_s16,
    int16x8_t,
    int16x8_t,
    uint16x8_t,
    "cmeq {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vceqq_s32,
    int32x4_t,
    int32x4_t,
    uint32x4_t,
    "cmeq {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vceqq_s8,
    int8x16_t,
    int8x16_t,
    uint8x16_t,
    "cmeq {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vcgeq_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "cmhs {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vcgtq_s16,
    int16x8_t,
    int16x8_t,
    uint16x8_t,
    "cmgt {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vcgtq_s32,
    int32x4_t,
    int32x4_t,
    uint32x4_t,
    "cmgt {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vcgtq_s64,
    int64x2_t,
    int64x2_t,
    uint64x2_t,
    "cmgt {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vcgtq_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "cmhi {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_unary!(
    vcgtzq_s16,
    int16x8_t,
    uint16x8_t,
    "cmgt {0:v}.8h, {1:v}.8h, #0"
);
neon_unary!(
    vcgtzq_s32,
    int32x4_t,
    uint32x4_t,
    "cmgt {0:v}.4s, {1:v}.4s, #0"
);
neon_unary!(
    vcgtzq_s64,
    int64x2_t,
    uint64x2_t,
    "cmgt {0:v}.2d, {1:v}.2d, #0"
);
neon_binary!(
    vcltq_s16,
    int16x8_t,
    int16x8_t,
    uint16x8_t,
    "cmgt {0:v}.8h, {2:v}.8h, {1:v}.8h"
);
neon_binary!(
    vcltq_s64,
    int64x2_t,
    int64x2_t,
    uint64x2_t,
    "cmgt {0:v}.2d, {2:v}.2d, {1:v}.2d"
);
neon_binary!(
    vcltq_s8,
    int8x16_t,
    int8x16_t,
    uint8x16_t,
    "cmgt {0:v}.16b, {2:v}.16b, {1:v}.16b"
);
neon_binary!(
    vcltq_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "cmhi {0:v}.16b, {2:v}.16b, {1:v}.16b"
);
neon_unary!(
    vcltzq_s16,
    int16x8_t,
    uint16x8_t,
    "cmlt {0:v}.8h, {1:v}.8h, #0"
);
neon_unary!(
    vcltzq_s32,
    int32x4_t,
    uint32x4_t,
    "cmlt {0:v}.4s, {1:v}.4s, #0"
);
neon_unary!(
    vcltzq_s64,
    int64x2_t,
    uint64x2_t,
    "cmlt {0:v}.2d, {1:v}.2d, #0"
);
neon_unary!(
    vcltzq_s8,
    int8x16_t,
    uint8x16_t,
    "cmlt {0:v}.16b, {1:v}.16b, #0"
);
neon_binary!(
    veorq_u16,
    uint16x8_t,
    uint16x8_t,
    uint16x8_t,
    "eor {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    veorq_u64,
    uint64x2_t,
    uint64x2_t,
    uint64x2_t,
    "eor {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    veorq_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "eor {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vmaxq_s16,
    int16x8_t,
    int16x8_t,
    int16x8_t,
    "smax {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vmaxq_s32,
    int32x4_t,
    int32x4_t,
    int32x4_t,
    "smax {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vmaxq_s8,
    int8x16_t,
    int8x16_t,
    int8x16_t,
    "smax {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vmaxq_u16,
    uint16x8_t,
    uint16x8_t,
    uint16x8_t,
    "umax {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vmaxq_u32,
    uint32x4_t,
    uint32x4_t,
    uint32x4_t,
    "umax {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vmaxq_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "umax {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_unary!(vmaxvq_s16, int16x8_t, i16, "smaxv {0:h}, {1:v}.8h");
neon_unary!(vmaxvq_s32, int32x4_t, i32, "smaxv {0:s}, {1:v}.4s");
neon_unary!(vmaxvq_s8, int8x16_t, i8, "smaxv {0:b}, {1:v}.16b");
neon_unary!(vmaxvq_u16, uint16x8_t, u16, "umaxv {0:h}, {1:v}.8h");
neon_binary!(
    vminq_s16,
    int16x8_t,
    int16x8_t,
    int16x8_t,
    "smin {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vminq_s32,
    int32x4_t,
    int32x4_t,
    int32x4_t,
    "smin {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vminq_s8,
    int8x16_t,
    int8x16_t,
    int8x16_t,
    "smin {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vminq_u16,
    uint16x8_t,
    uint16x8_t,
    uint16x8_t,
    "umin {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vminq_u32,
    uint32x4_t,
    uint32x4_t,
    uint32x4_t,
    "umin {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vminq_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "umin {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_unary!(vminvq_s16, int16x8_t, i16, "sminv {0:h}, {1:v}.8h");
neon_unary!(vminvq_s32, int32x4_t, i32, "sminv {0:s}, {1:v}.4s");
neon_unary!(vminvq_s8, int8x16_t, i8, "sminv {0:b}, {1:v}.16b");
neon_unary!(vminvq_u16, uint16x8_t, u16, "uminv {0:h}, {1:v}.8h");
neon_unary!(vminvq_u32, uint32x4_t, u32, "uminv {0:s}, {1:v}.4s");
neon_unary!(vminvq_u8, uint8x16_t, u8, "uminv {0:b}, {1:v}.16b");
neon_unary!(
    vmovl_s32,
    int32x2_t,
    int64x2_t,
    "sshll {0:v}.2d, {1:v}.2s, #0"
);
neon_binary!(
    vmull_s16,
    int16x4_t,
    int16x4_t,
    int32x4_t,
    "smull {0:v}.4s, {1:v}.4h, {2:v}.4h"
);
neon_binary!(
    vmull_s32,
    int32x2_t,
    int32x2_t,
    int64x2_t,
    "smull {0:v}.2d, {1:v}.2s, {2:v}.2s"
);
neon_binary!(
    vmull_s8,
    int8x8_t,
    int8x8_t,
    int16x8_t,
    "smull {0:v}.8h, {1:v}.8b, {2:v}.8b"
);
neon_binary!(
    vmull_u16,
    uint16x4_t,
    uint16x4_t,
    uint32x4_t,
    "umull {0:v}.4s, {1:v}.4h, {2:v}.4h"
);
neon_binary!(
    vmull_u32,
    uint32x2_t,
    uint32x2_t,
    uint64x2_t,
    "umull {0:v}.2d, {1:v}.2s, {2:v}.2s"
);
neon_binary!(
    vmull_u8,
    uint8x8_t,
    uint8x8_t,
    uint16x8_t,
    "umull {0:v}.8h, {1:v}.8b, {2:v}.8b"
);
neon_binary!(
    vmulq_u16,
    uint16x8_t,
    uint16x8_t,
    uint16x8_t,
    "mul {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vmulq_u32,
    uint32x4_t,
    uint32x4_t,
    uint32x4_t,
    "mul {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vmulq_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "mul {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_unary!(vnegq_s16, int16x8_t, int16x8_t, "neg {0:v}.8h, {1:v}.8h");
neon_unary!(vnegq_s32, int32x4_t, int32x4_t, "neg {0:v}.4s, {1:v}.4s");
neon_unary!(vnegq_s64, int64x2_t, int64x2_t, "neg {0:v}.2d, {1:v}.2d");
neon_unary!(vnegq_s8, int8x16_t, int8x16_t, "neg {0:v}.16b, {1:v}.16b");
neon_binary!(
    vorrq_u64,
    uint64x2_t,
    uint64x2_t,
    uint64x2_t,
    "orr {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vpaddq_s32,
    int32x4_t,
    int32x4_t,
    int32x4_t,
    "addp {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vqaddq_s16,
    int16x8_t,
    int16x8_t,
    int16x8_t,
    "sqadd {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vqaddq_s32,
    int32x4_t,
    int32x4_t,
    int32x4_t,
    "sqadd {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vqaddq_s64,
    int64x2_t,
    int64x2_t,
    int64x2_t,
    "sqadd {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vqaddq_s8,
    int8x16_t,
    int8x16_t,
    int8x16_t,
    "sqadd {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vqaddq_u16,
    uint16x8_t,
    uint16x8_t,
    uint16x8_t,
    "uqadd {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vqaddq_u32,
    uint32x4_t,
    uint32x4_t,
    uint32x4_t,
    "uqadd {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vqaddq_u64,
    uint64x2_t,
    uint64x2_t,
    uint64x2_t,
    "uqadd {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vqaddq_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "uqadd {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_unary!(vqmovn_s16, int16x8_t, int8x8_t, "sqxtn {0:v}.8b, {1:v}.8h");
neon_unary!(vqmovn_s32, int32x4_t, int16x4_t, "sqxtn {0:v}.4h, {1:v}.4s");
neon_unary!(
    vqmovun_s16,
    int16x8_t,
    uint8x8_t,
    "sqxtun {0:v}.8b, {1:v}.8h"
);
neon_binary!(
    vqrdmulhq_s16,
    int16x8_t,
    int16x8_t,
    int16x8_t,
    "sqrdmulh {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vqsubq_s16,
    int16x8_t,
    int16x8_t,
    int16x8_t,
    "sqsub {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vqsubq_s32,
    int32x4_t,
    int32x4_t,
    int32x4_t,
    "sqsub {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vqsubq_s64,
    int64x2_t,
    int64x2_t,
    int64x2_t,
    "sqsub {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vqsubq_s8,
    int8x16_t,
    int8x16_t,
    int8x16_t,
    "sqsub {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vqsubq_u16,
    uint16x8_t,
    uint16x8_t,
    uint16x8_t,
    "uqsub {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vqsubq_u32,
    uint32x4_t,
    uint32x4_t,
    uint32x4_t,
    "uqsub {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vqsubq_u64,
    uint64x2_t,
    uint64x2_t,
    uint64x2_t,
    "uqsub {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_unary!(
    vrev64q_u16,
    uint16x8_t,
    uint16x8_t,
    "rev64 {0:v}.8h, {1:v}.8h"
);
neon_unary!(
    vrev64q_u32,
    uint32x4_t,
    uint32x4_t,
    "rev64 {0:v}.4s, {1:v}.4s"
);
neon_binary!(
    vshlq_s16,
    int16x8_t,
    int16x8_t,
    int16x8_t,
    "sshl {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vshlq_s32,
    int32x4_t,
    int32x4_t,
    int32x4_t,
    "sshl {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vshlq_s64,
    int64x2_t,
    int64x2_t,
    int64x2_t,
    "sshl {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vshlq_s8,
    int8x16_t,
    int8x16_t,
    int8x16_t,
    "sshl {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vshlq_u16,
    uint16x8_t,
    int16x8_t,
    uint16x8_t,
    "ushl {0:v}.8h, {1:v}.8h, {2:v}.8h"
);
neon_binary!(
    vshlq_u32,
    uint32x4_t,
    int32x4_t,
    uint32x4_t,
    "ushl {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vshlq_u64,
    uint64x2_t,
    int64x2_t,
    uint64x2_t,
    "ushl {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vshlq_u8,
    uint8x16_t,
    int8x16_t,
    uint8x16_t,
    "ushl {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vzip1q_s64,
    int64x2_t,
    int64x2_t,
    int64x2_t,
    "zip1 {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vzip1q_u32,
    uint32x4_t,
    uint32x4_t,
    uint32x4_t,
    "zip1 {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vzip1q_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "zip1 {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vzip1q_u64,
    uint64x2_t,
    uint64x2_t,
    uint64x2_t,
    "zip1 {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vzip2q_s64,
    int64x2_t,
    int64x2_t,
    int64x2_t,
    "zip2 {0:v}.2d, {1:v}.2d, {2:v}.2d"
);
neon_binary!(
    vzip2q_u32,
    uint32x4_t,
    uint32x4_t,
    uint32x4_t,
    "zip2 {0:v}.4s, {1:v}.4s, {2:v}.4s"
);
neon_binary!(
    vzip2q_u8,
    uint8x16_t,
    uint8x16_t,
    uint8x16_t,
    "zip2 {0:v}.16b, {1:v}.16b, {2:v}.16b"
);
neon_binary!(
    vmull_high_s16,
    int16x8_t,
    int16x8_t,
    int32x4_t,
    "smull2 {0:v}.4s, {1:v}.8h, {2:v}.8h"
);

/// Multiplies by a broadcast halfword with signed rounding and saturation.
#[inline]
#[target_feature(enable = "neon")]
pub fn vqrdmulhq_n_s16(a: int16x8_t, b: i16) -> int16x8_t {
    vqrdmulhq_s16(a, int16x8_t([b; 8]))
}
