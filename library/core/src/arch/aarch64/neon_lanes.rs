//! AArch64 lane manipulation, selections and bit casts.
use super::*;
use crate::arch::asm;

macro_rules! combine {
    ($name:ident, $half:ident, $full:ident, $lanes:literal, $total:literal) => {
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name(low: $half, high: $half) -> $full {
            let mut result = [low.0[0]; $total];
            let mut i = 0;
            while i < $lanes {
                result[i] = low.0[i];
                result[$lanes + i] = high.0[i];
                i += 1;
            }
            $full(result)
        }
    };
}

macro_rules! half {
    ($name:ident, $full:ident, $half:ident, $start:literal, $lanes:literal) => {
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name(value: $full) -> $half {
            let mut result = [value.0[$start]; $lanes];
            let mut i = 0;
            while i < $lanes {
                result[i] = value.0[$start + i];
                i += 1;
            }
            $half(result)
        }
    };
}

macro_rules! lane {
    ($name:ident, $input:ident, $scalar:ty) => {
        #[rustc_legacy_const_generics(1)]
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name<const LANE: i32>(value: $input) -> $scalar {
            value.0[LANE as usize]
        }
    };
}

macro_rules! broadcast {
    ($name:ident, $scalar:ty, $output:ident, $lanes:literal) => {
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name(value: $scalar) -> $output {
            $output([value; $lanes])
        }
    };
}

macro_rules! load {
    ($name:ident, $scalar:ty, $output:ident) => {
        /// Loads a vector's worth of consecutive lanes.
        ///
        /// # Safety
        /// The pointer must address enough initialized lanes for the vector.
        #[inline]
        #[target_feature(enable = "neon")]
        pub unsafe fn $name(pointer: *const $scalar) -> $output {
            // SAFETY: the vector and its lane array have identical storage.
            unsafe { crate::ptr::read_unaligned(pointer.cast::<$output>()) }
        }
    };
}

macro_rules! bit_select {
    ($name:ident, $mask:ident, $vector:ident) => {
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name(mask: $mask, a: $vector, b: $vector) -> $vector {
            let result;
            // SAFETY: both vectors have identical storage and all bit
            // patterns are valid. The instruction updates only the mask.
            unsafe {
                let mask = crate::mem::transmute::<$mask, $vector>(mask);
                asm!("bsl {0:v}.16b, {1:v}.16b, {2:v}.16b",
                     inout(vreg) mask => result, in(vreg) a, in(vreg) b,
                     options(nomem, nostack));
            }
            result
        }
    };
}

macro_rules! permute_pair {
    ($name:ident, $vector:ident, $pair:ident, $first:literal, $second:literal) => {
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name(a: $vector, b: $vector) -> $pair {
            let low;
            let high;
            // SAFETY: the distinct outputs cannot overlap the two inputs,
            // which both instructions read.
            unsafe {
                asm!($first, $second, out(vreg) low, out(vreg) high, in(vreg) a, in(vreg) b,
                     options(nomem, nostack));
            }
            $pair(low, high)
        }
    };
}

macro_rules! table_lookup {
    ($name:ident, $pair:ident, $instruction:literal $(, $register:literal, $index:tt)+) => {
        #[inline]
        #[target_feature(enable = "neon")]
        pub fn $name(table: $pair, indices: uint8x16_t) -> uint8x16_t {
            let result;
            // SAFETY: the instruction reads the given consecutive registers
            // and returns zero for out-of-range indices.
            unsafe {
                asm!($instruction, lateout(vreg) result, in(vreg) indices,
                     $(in($register) table.$index,)+ options(nomem, nostack));
            }
            result
        }
    };
}

broadcast!(vdupq_n_s64, i64, int64x2_t, 2);
pub use self::vdupq_n_s64 as vmovq_n_s64;
pub use super::vdupq_n_s32 as vmovq_n_s32;
pub use super::vdupq_n_u8 as vmovq_n_u8;

#[inline]
#[target_feature(enable = "neon")]
pub fn vqtbl1q_s8(table: int8x16_t, indices: uint8x16_t) -> int8x16_t {
    vreinterpretq_s8_u8(vqtbl1q_u8(vreinterpretq_u8_s8(table), indices))
}

table_lookup!(
    vqtbl2q_u8,
    uint8x16x2_t,
    "tbl {0:v}.16b, {{v0.16b, v1.16b}}, {1:v}.16b",
    "v0",
    0,
    "v1",
    1
);
table_lookup!(
    vqtbl3q_u8,
    uint8x16x3_t,
    "tbl {0:v}.16b, {{v0.16b, v1.16b, v2.16b}}, {1:v}.16b",
    "v0",
    0,
    "v1",
    1,
    "v2",
    2
);
table_lookup!(
    vqtbl4q_u8,
    uint8x16x4_t,
    "tbl {0:v}.16b, {{v0.16b, v1.16b, v2.16b, v3.16b}}, {1:v}.16b",
    "v0",
    0,
    "v1",
    1,
    "v2",
    2,
    "v3",
    3
);

/// Extracts eight lanes from the concatenation of `a` and `b`.
#[rustc_legacy_const_generics(2)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vextq_u16<const N: i32>(a: uint16x8_t, b: uint16x8_t) -> uint16x8_t {
    let mut lanes = [0; 8];
    let mut i = 0;
    while i < 8 {
        let start = i + N as usize;
        lanes[i] = if start < 8 {
            a.0[start]
        } else {
            b.0[start - 8]
        };
        i += 1;
    }
    uint16x8_t(lanes)
}

/// Shifts each lane right before truncating it to a word.
#[rustc_legacy_const_generics(1)]
#[inline]
#[target_feature(enable = "neon")]
pub fn vshrn_n_u64<const N: i32>(a: uint64x2_t) -> uint32x2_t {
    uint32x2_t([(a.0[0] >> N) as u32, (a.0[1] >> N) as u32])
}

permute_pair!(
    vuzpq_s16,
    int16x8_t,
    int16x8x2_t,
    "uzp1 {0:v}.8h, {2:v}.8h, {3:v}.8h",
    "uzp2 {1:v}.8h, {2:v}.8h, {3:v}.8h"
);
permute_pair!(
    vuzpq_s32,
    int32x4_t,
    int32x4x2_t,
    "uzp1 {0:v}.4s, {2:v}.4s, {3:v}.4s",
    "uzp2 {1:v}.4s, {2:v}.4s, {3:v}.4s"
);
permute_pair!(
    vuzpq_s8,
    int8x16_t,
    int8x16x2_t,
    "uzp1 {0:v}.16b, {2:v}.16b, {3:v}.16b",
    "uzp2 {1:v}.16b, {2:v}.16b, {3:v}.16b"
);
permute_pair!(
    vuzpq_u16,
    uint16x8_t,
    uint16x8x2_t,
    "uzp1 {0:v}.8h, {2:v}.8h, {3:v}.8h",
    "uzp2 {1:v}.8h, {2:v}.8h, {3:v}.8h"
);
permute_pair!(
    vuzpq_u32,
    uint32x4_t,
    uint32x4x2_t,
    "uzp1 {0:v}.4s, {2:v}.4s, {3:v}.4s",
    "uzp2 {1:v}.4s, {2:v}.4s, {3:v}.4s"
);
permute_pair!(
    vuzpq_u8,
    uint8x16_t,
    uint8x16x2_t,
    "uzp1 {0:v}.16b, {2:v}.16b, {3:v}.16b",
    "uzp2 {1:v}.16b, {2:v}.16b, {3:v}.16b"
);
bit_select!(vbslq_f64, uint64x2_t, float64x2_t);
bit_select!(vbslq_u16, uint16x8_t, uint16x8_t);
bit_select!(vbslq_u32, uint32x4_t, uint32x4_t);
bit_select!(vbslq_u64, uint64x2_t, uint64x2_t);
combine!(vcombine_s16, int16x4_t, int16x8_t, 4, 8);
combine!(vcombine_s8, int8x8_t, int8x16_t, 8, 16);
combine!(vcombine_u16, uint16x4_t, uint16x8_t, 4, 8);
combine!(vcombine_u32, uint32x2_t, uint32x4_t, 2, 4);
combine!(vcombine_u8, uint8x8_t, uint8x16_t, 8, 16);
half!(vget_high_s16, int16x8_t, int16x4_t, 4, 4);
half!(vget_high_s32, int32x4_t, int32x2_t, 2, 2);
half!(vget_high_s8, int8x16_t, int8x8_t, 8, 8);
half!(vget_high_u16, uint16x8_t, uint16x4_t, 4, 4);
half!(vget_high_u32, uint32x4_t, uint32x2_t, 2, 2);
half!(vget_high_u64, uint64x2_t, uint64x1_t, 1, 1);
half!(vget_high_u8, uint8x16_t, uint8x8_t, 8, 8);
half!(vget_low_s16, int16x8_t, int16x4_t, 0, 4);
half!(vget_low_s32, int32x4_t, int32x2_t, 0, 2);
half!(vget_low_s8, int8x16_t, int8x8_t, 0, 8);
half!(vget_low_u16, uint16x8_t, uint16x4_t, 0, 4);
half!(vget_low_u32, uint32x4_t, uint32x2_t, 0, 2);
half!(vget_low_u64, uint64x2_t, uint64x1_t, 0, 1);
lane!(vgetq_lane_f64, float64x2_t, f64);
lane!(vgetq_lane_s64, int64x2_t, i64);
lane!(vgetq_lane_s8, int8x16_t, i8);
lane!(vgetq_lane_u16, uint16x8_t, u16);
lane!(vgetq_lane_u8, uint8x16_t, u8);
load!(vld1q_s16, i16, int16x8_t);
load!(vld1q_s8, i8, int8x16_t);
broadcast!(vdupq_n_s16, i16, int16x8_t, 8);
pub use self::vdupq_n_s16 as vmovq_n_s16;
broadcast!(vmovq_n_s8, i8, int8x16_t, 16);
broadcast!(vmovq_n_u16, u16, uint16x8_t, 8);
broadcast!(vmovq_n_u32, u32, uint32x4_t, 4);
broadcast!(vmovq_n_u64, u64, uint64x2_t, 2);
permute_pair!(
    vtrnq_u16,
    uint16x8_t,
    uint16x8x2_t,
    "trn1 {0:v}.8h, {2:v}.8h, {3:v}.8h",
    "trn2 {1:v}.8h, {2:v}.8h, {3:v}.8h"
);
permute_pair!(
    vtrnq_u32,
    uint32x4_t,
    uint32x4x2_t,
    "trn1 {0:v}.4s, {2:v}.4s, {3:v}.4s",
    "trn2 {1:v}.4s, {2:v}.4s, {3:v}.4s"
);
permute_pair!(
    vzip_u8,
    uint8x8_t,
    uint8x8x2_t,
    "zip1 {0:v}.8b, {2:v}.8b, {3:v}.8b",
    "zip2 {1:v}.8b, {2:v}.8b, {3:v}.8b"
);

reinterpret128!(vreinterpretq_f32_s32, int32x4_t, float32x4_t);
reinterpret128!(vreinterpretq_f64_s64, int64x2_t, float64x2_t);
reinterpret128!(vreinterpretq_f64_u32, uint32x4_t, float64x2_t);
reinterpret128!(vreinterpretq_f64_u64, uint64x2_t, float64x2_t);
reinterpret128!(vreinterpretq_s16_u16, uint16x8_t, int16x8_t);
reinterpret128!(vreinterpretq_s32_s64, int64x2_t, int32x4_t);
reinterpret128!(vreinterpretq_s64_u64, uint64x2_t, int64x2_t);
reinterpret128!(vreinterpretq_s8_s16, int16x8_t, int8x16_t);
reinterpret128!(vreinterpretq_u16_s16, int16x8_t, uint16x8_t);
reinterpret128!(vreinterpretq_u16_s32, int32x4_t, uint16x8_t);
reinterpret128!(vreinterpretq_u32_f64, float64x2_t, uint32x4_t);
reinterpret128!(vreinterpretq_u32_s32, int32x4_t, uint32x4_t);
reinterpret128!(vreinterpretq_u64_f64, float64x2_t, uint64x2_t);
reinterpret128!(vreinterpretq_u8_s8, int8x16_t, uint8x16_t);

reinterpret128!(vreinterpretq_s64_s32, int32x4_t, int64x2_t);
permute_pair!(
    vtrnq_s32,
    int32x4_t,
    int32x4x2_t,
    "trn1 {0:v}.4s, {2:v}.4s, {3:v}.4s",
    "trn2 {1:v}.4s, {2:v}.4s, {3:v}.4s"
);

#[inline]
#[target_feature(enable = "neon")]
pub fn vcreate_s16(bits: u64) -> int16x4_t {
    int16x4_t([bits as i16, (bits >> 16) as i16, (bits >> 32) as i16, (bits >> 48) as i16])
}

macro_rules! load_pair {
    ($name:ident, $element:ty, $pair:ident, $load:ident, $lanes:literal) => {
        /// Loads two consecutive vectors.
        ///
        /// # Safety
        /// The pointer must address two vectors' worth of initialized elements.
        #[inline]
        #[target_feature(enable = "neon")]
        pub unsafe fn $name(pointer: *const $element) -> $pair {
            // SAFETY: both loads are within the caller's readable range.
            unsafe { $pair($load(pointer), $load(pointer.add($lanes))) }
        }
    };
}

load_pair!(vld1q_s16_x2, i16, int16x8x2_t, vld1q_s16, 8);
load_pair!(vld1q_s32_x2, i32, int32x4x2_t, vld1q_s32, 4);

/// Stores eight consecutive halfwords.
///
/// # Safety
/// The pointer must address eight writable halfwords.
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vst1q_s16(pointer: *mut i16, value: int16x8_t) {
    // SAFETY: the caller supplies space for the complete lane array.
    unsafe { crate::ptr::write_unaligned(pointer.cast::<[i16; 8]>(), value.0) }
}

/// Stores two consecutive halfword vectors.
///
/// # Safety
/// The pointer must address sixteen writable halfwords.
#[inline]
#[target_feature(enable = "neon")]
pub unsafe fn vst1q_s16_x2(pointer: *mut i16, value: int16x8x2_t) {
    // SAFETY: both stores are within the caller's writable range.
    unsafe {
        vst1q_s16(pointer, value.0);
        vst1q_s16(pointer.add(8), value.1);
    }
}

macro_rules! store_interleaved {
    ($name:ident, $input:ty, $instruction:literal $(, $register:literal, $channel:tt)+) => {
        /// Stores lanes with the supplied channels interleaved in memory.
        ///
        /// # Safety
        /// The pointer must address sixteen writable bytes per channel.
        #[inline]
        #[target_feature(enable = "neon")]
        pub unsafe fn $name(pointer: *mut u8, value: $input) {
            // SAFETY: the caller provides the writable range, and the
            // instruction reads its channels from the given consecutive registers.
            unsafe {
                asm!($instruction, in(reg) pointer, $(in($register) value.$channel,)+
                     options(nostack, preserves_flags));
            }
        }
    };
}

store_interleaved!(vst3q_u8, uint8x16x3_t,
    "st3 {{v0.16b, v1.16b, v2.16b}}, [{0}]", "v0", 0, "v1", 1, "v2", 2);
store_interleaved!(vst4q_u8, uint8x16x4_t,
    "st4 {{v0.16b, v1.16b, v2.16b, v3.16b}}, [{0}]", "v0", 0, "v1", 1, "v2", 2, "v3", 3);
