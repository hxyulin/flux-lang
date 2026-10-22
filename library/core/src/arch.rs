//! A bounded AArch64 NEON foundation. NEON is baseline on this target.

pub mod aarch64 {
    /// Sixteen unsigned byte lanes, with Rust's NEON size and alignment.
    #[allow(non_camel_case_types)]
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    pub struct uint8x16_t(pub(crate) [u8; 16]);

    /// Eight unsigned halfword lanes.
    #[allow(non_camel_case_types)]
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    pub struct uint16x8_t(pub(crate) [u16; 8]);

    /// Two unsigned doubleword lanes.
    #[allow(non_camel_case_types)]
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    pub struct uint64x2_t(pub(crate) [u64; 2]);

    /// Eight unsigned byte lanes.
    #[allow(non_camel_case_types)]
    #[repr(C, align(8))]
    #[derive(Clone, Copy)]
    pub struct uint8x8_t(pub(crate) [u8; 8]);

    /// One unsigned doubleword lane.
    #[allow(non_camel_case_types)]
    #[repr(C, align(8))]
    #[derive(Clone, Copy)]
    pub struct uint64x1_t(pub(crate) [u64; 1]);

    /// Reads sixteen readable bytes, without an alignment requirement.
    pub unsafe fn vld1q_u8(ptr: *const u8) -> uint8x16_t {
        unsafe { __neon_vld1q_u8(ptr) }
    }

    /// Duplicates a byte into all sixteen lanes.
    pub unsafe fn vdupq_n_u8(value: u8) -> uint8x16_t {
        __neon_vdupq_n_u8(value)
    }

    /// Compares lanes, producing 255 for equal lanes and zero otherwise.
    pub unsafe fn vceqq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        __neon_vceqq_u8(a, b)
    }

    /// Bitwise intersection.
    pub unsafe fn vandq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        __neon_vandq_u8(a, b)
    }

    /// Bitwise union.
    pub unsafe fn vorrq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        __neon_vorrq_u8(a, b)
    }

    /// Reinterprets all bits as eight halfwords.
    pub unsafe fn vreinterpretq_u16_u8(a: uint8x16_t) -> uint16x8_t {
        // SAFETY: equal sizes and alignments; all destination bits are valid.
        unsafe { *(&a as *const uint8x16_t as *const uint16x8_t) }
    }

    /// Reinterprets all bits as two doublewords.
    pub unsafe fn vreinterpretq_u64_u8(a: uint8x16_t) -> uint64x2_t {
        // SAFETY: equal sizes and alignments; all destination bits are valid.
        unsafe { *(&a as *const uint8x16_t as *const uint64x2_t) }
    }

    /// Reinterprets all bits as one doubleword.
    pub unsafe fn vreinterpret_u64_u8(a: uint8x8_t) -> uint64x1_t {
        // SAFETY: equal sizes and alignments; all destination bits are valid.
        unsafe { *(&a as *const uint8x8_t as *const uint64x1_t) }
    }

    /// Extracts lane 0 or 1. The compiler checks the constant lane.
    #[rustc_legacy_const_generics(1)]
    pub unsafe fn vgetq_lane_u64<const LANE: i32>(a: uint64x2_t) -> u64 {
        __neon_vgetq_lane_u64::<LANE>(a)
    }

    /// Extracts lane 0. The compiler checks the constant lane.
    #[rustc_legacy_const_generics(1)]
    pub unsafe fn vget_lane_u64<const LANE: i32>(a: uint64x1_t) -> u64 {
        __neon_vget_lane_u64::<LANE>(a)
    }

    /// Shifts each halfword right by N bits, keeping its low byte.
    /// N must be a compile-time constant in 1..=8.
    ///
    /// # Safety
    /// The caller must run on an AArch64 target with NEON.
    #[rustc_legacy_const_generics(1)]
    pub unsafe fn vshrn_n_u16<const N: i32>(a: uint16x8_t) -> uint8x8_t {
        __neon_vshrn_n_u16::<N>(a)
    }

    #[rustc_intrinsic]
    fn __neon_vshrn_n_u16<const N: i32>(a: uint16x8_t) -> uint8x8_t {
        loop {}
    }

    /// Pairwise unsigned maxima of a, followed by those of b.
    ///
    /// # Safety
    /// The caller must run on an AArch64 target with NEON.
    pub unsafe fn vpmaxq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        __neon_vpmaxq_u8(a, b)
    }

    #[rustc_intrinsic]
    fn __neon_vpmaxq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        loop {}
    }

    impl crate::fmt::Debug for uint8x16_t {
        fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
            let mut tuple = f.debug_tuple("uint8x16_t");
            let mut i = 0;
            while i < 16 {
                tuple.field(&self.0[i]);
                i += 1;
            }
            tuple.finish()
        }
    }

    impl crate::fmt::Debug for uint16x8_t {
        fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
            let mut tuple = f.debug_tuple("uint16x8_t");
            let mut i = 0;
            while i < 8 {
                tuple.field(&self.0[i]);
                i += 1;
            }
            tuple.finish()
        }
    }

    impl crate::fmt::Debug for uint64x2_t {
        fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
            let mut tuple = f.debug_tuple("uint64x2_t");
            let mut i = 0;
            while i < 2 {
                tuple.field(&self.0[i]);
                i += 1;
            }
            tuple.finish()
        }
    }

    impl crate::fmt::Debug for uint8x8_t {
        fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
            let mut tuple = f.debug_tuple("uint8x8_t");
            let mut i = 0;
            while i < 8 {
                tuple.field(&self.0[i]);
                i += 1;
            }
            tuple.finish()
        }
    }

    impl crate::fmt::Debug for uint64x1_t {
        fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
            let mut tuple = f.debug_tuple("uint64x1_t");
            let mut i = 0;
            while i < 1 {
                tuple.field(&self.0[i]);
                i += 1;
            }
            tuple.finish()
        }
    }

    // Public wrappers remain callable when reified as unsafe function pointers.
    #[rustc_intrinsic]
    unsafe fn __neon_vld1q_u8(ptr: *const u8) -> uint8x16_t {
        loop {}
    }

    #[rustc_intrinsic]
    fn __neon_vdupq_n_u8(value: u8) -> uint8x16_t {
        loop {}
    }

    #[rustc_intrinsic]
    fn __neon_vceqq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        loop {}
    }

    #[rustc_intrinsic]
    fn __neon_vandq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        loop {}
    }

    #[rustc_intrinsic]
    fn __neon_vorrq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        loop {}
    }

    #[rustc_intrinsic]
    fn __neon_vgetq_lane_u64<const LANE: i32>(a: uint64x2_t) -> u64 {
        loop {}
    }

    #[rustc_intrinsic]
    fn __neon_vget_lane_u64<const LANE: i32>(a: uint64x1_t) -> u64 {
        loop {}
    }
}
