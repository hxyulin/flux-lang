//! A bounded AArch64 NEON foundation. NEON is baseline on this target.

pub mod aarch64 {
    /// Accumulates one byte with the reflected IEEE CRC-32 polynomial.
    ///
    /// This scalar implementation preserves the instruction's uncomplemented
    /// state. It does not provide hardware acceleration.
    ///
    /// # Safety
    /// The caller must run on an AArch64 target with the CRC feature enabled.
    #[target_feature(enable = "crc")]
    pub fn __crc32b(crc: u32, byte: u8) -> u32 {
        let mut crc = crc ^ byte as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = (crc >> 1) ^ (0xedb8_8320u32 & 0u32.wrapping_sub(crc & 1));
            bit += 1;
        }
        crc
    }

    /// Accumulates a doubleword, starting with its least significant byte.
    ///
    /// # Safety
    /// The caller must run on an AArch64 target with the CRC feature enabled.
    #[target_feature(enable = "crc")]
    pub fn __crc32d(crc: u32, value: u64) -> u32 {
        let mut crc = crc;
        let mut value = value;
        let mut byte = 0;
        while byte < 8 {
            // SAFETY: the caller establishes the same CRC feature requirement.
            crc = unsafe { __crc32b(crc, value as u8) };
            value >>= 8;
            byte += 1;
        }
        crc
    }

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

    /// Four unsigned word lanes, with the Rust NEON size and alignment.
    #[allow(non_camel_case_types)]
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    pub struct uint32x4_t(pub(crate) [u32; 4]);

    /// Adds corresponding lanes modulo 2^32.
    #[target_feature(enable = "neon")]
    pub fn vaddq_u32(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        uint32x4_t([
            a.0[0].wrapping_add(b.0[0]),
            a.0[1].wrapping_add(b.0[1]),
            a.0[2].wrapping_add(b.0[2]),
            a.0[3].wrapping_add(b.0[3]),
        ])
    }

    /// Adds corresponding lanes modulo 2^64.
    #[target_feature(enable = "neon")]
    pub fn vaddq_u64(a: uint64x2_t, b: uint64x2_t) -> uint64x2_t {
        uint64x2_t([a.0[0].wrapping_add(b.0[0]), a.0[1].wrapping_add(b.0[1])])
    }

    /// Reads four words, without requiring pointer alignment.
    ///
    /// # Safety
    /// `ptr` must address sixteen readable initialized bytes.
    pub unsafe fn vld1q_u32(ptr: *const u32) -> uint32x4_t {
        // SAFETY: each unaligned word is within the caller's readable region.
        unsafe {
            uint32x4_t([
                crate::ptr::read_unaligned(ptr),
                crate::ptr::read_unaligned(ptr.add(1)),
                crate::ptr::read_unaligned(ptr.add(2)),
                crate::ptr::read_unaligned(ptr.add(3)),
            ])
        }
    }

    /// Reads two doublewords, without requiring pointer alignment.
    ///
    /// # Safety
    /// `ptr` must address sixteen readable initialized bytes.
    pub unsafe fn vld1q_u64(ptr: *const u64) -> uint64x2_t {
        // SAFETY: both unaligned words are within the caller's readable region.
        unsafe {
            uint64x2_t([
                crate::ptr::read_unaligned(ptr),
                crate::ptr::read_unaligned(ptr.add(1)),
            ])
        }
    }

    /// Writes four words without requiring pointer alignment.
    ///
    /// # Safety
    /// `ptr` must address sixteen writable bytes.
    pub unsafe fn vst1q_u32(ptr: *mut u32, a: uint32x4_t) {
        let mut i = 0;
        while i < 4 {
            // SAFETY: every unaligned word is in the caller's writable region.
            unsafe {
                crate::ptr::write_unaligned(ptr.add(i), a.0[i]);
            }
            i += 1;
        }
    }

    /// Writes two doublewords without requiring pointer alignment.
    ///
    /// # Safety
    /// `ptr` must address sixteen writable bytes.
    pub unsafe fn vst1q_u64(ptr: *mut u64, a: uint64x2_t) {
        let mut i = 0;
        while i < 2 {
            // SAFETY: every unaligned word is in the caller's writable region.
            unsafe {
                crate::ptr::write_unaligned(ptr.add(i), a.0[i]);
            }
            i += 1;
        }
    }

    /// Reverses bytes independently inside each word.
    #[target_feature(enable = "neon")]
    pub fn vrev32q_u8(a: uint8x16_t) -> uint8x16_t {
        let mut result = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            result[i] = a.0[(i & !3usize) | (3 - (i & 3))];
            i += 1;
        }
        uint8x16_t(result)
    }

    /// Reverses bytes independently inside each doubleword.
    #[target_feature(enable = "neon")]
    pub fn vrev64q_u8(a: uint8x16_t) -> uint8x16_t {
        let mut result = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            result[i] = a.0[(i & !7usize) | (7 - (i & 7))];
            i += 1;
        }
        uint8x16_t(result)
    }

    /// Reinterprets the same sixteen bytes as four words.
    #[target_feature(enable = "neon")]
    pub fn vreinterpretq_u32_u8(a: uint8x16_t) -> uint32x4_t {
        // SAFETY: size/alignment agree and every destination bit pattern is valid.
        unsafe { *(&a as *const uint8x16_t as *const uint32x4_t) }
    }

    /// Extracts two lanes from the concatenation of `a` and `b`.
    #[rustc_legacy_const_generics(2)]
    #[target_feature(enable = "neon")]
    pub fn vextq_u64<const N: i32>(a: uint64x2_t, b: uint64x2_t) -> uint64x2_t {
        // The existing lane intrinsic checks N after monomorphization,
        // including when this wrapper is reified as a function pointer.
        let first = __neon_vgetq_lane_u64::<N>(a);
        uint64x2_t([first, if N == 0 { a.0[1] } else { b.0[0] }])
    }

    // These scalar implementations preserve the Arm SHA instruction results,
    // including lane ordering and wrapping arithmetic; they do not accelerate
    // hashing. See Arm DDI 0602, SHA256H/H2/SU0/SU1 and SHA512H/H2/SU0/SU1:
    // https://documentation-service.arm.com/static/67e40f3398aa3c3b6eea6a85
    fn sha256_rounds(
        abcd: uint32x4_t,
        efgh: uint32x4_t,
        wk: uint32x4_t,
    ) -> (uint32x4_t, uint32x4_t) {
        let mut x = abcd.0;
        let mut y = efgh.0;
        let mut i = 0;
        while i < 4 {
            let e = y[0];
            let a = x[0];
            let choose = (e & y[1]) ^ (!e & y[2]);
            let majority = (a & x[1]) ^ (a & x[2]) ^ (x[1] & x[2]);
            let t = y[3]
                .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                .wrapping_add(choose)
                .wrapping_add(wk.0[i]);
            let next_a = t
                .wrapping_add(a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
                .wrapping_add(majority);
            let next_e = t.wrapping_add(x[3]);
            x = [next_a, x[0], x[1], x[2]];
            y = [next_e, y[0], y[1], y[2]];
            i += 1;
        }
        (uint32x4_t(x), uint32x4_t(y))
    }

    /// Runs four SHA-256 rounds, returning the updated a/b/c/d lanes.
    ///
    /// # Safety
    /// The caller must enable the SHA2 target feature.
    #[target_feature(enable = "sha2")]
    pub fn vsha256hq_u32(abcd: uint32x4_t, efgh: uint32x4_t, wk: uint32x4_t) -> uint32x4_t {
        sha256_rounds(abcd, efgh, wk).0
    }

    /// Runs four SHA-256 rounds, returning the updated e/f/g/h lanes.
    ///
    /// # Safety
    /// The caller must enable the SHA2 target feature.
    #[target_feature(enable = "sha2")]
    pub fn vsha256h2q_u32(efgh: uint32x4_t, abcd: uint32x4_t, wk: uint32x4_t) -> uint32x4_t {
        sha256_rounds(abcd, efgh, wk).1
    }

    fn sha256_small0(x: u32) -> u32 {
        x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)
    }
    fn sha256_small1(x: u32) -> u32 {
        x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10)
    }

    /// Performs the first SHA-256 schedule step on four lanes.
    ///
    /// # Safety
    /// The caller must enable the SHA2 target feature.
    #[target_feature(enable = "sha2")]
    pub fn vsha256su0q_u32(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        uint32x4_t([
            a.0[0].wrapping_add(sha256_small0(a.0[1])),
            a.0[1].wrapping_add(sha256_small0(a.0[2])),
            a.0[2].wrapping_add(sha256_small0(a.0[3])),
            a.0[3].wrapping_add(sha256_small0(b.0[0])),
        ])
    }

    /// Completes four SHA-256 schedule lanes, including their dependencies.
    ///
    /// # Safety
    /// The caller must enable the SHA2 target feature.
    #[target_feature(enable = "sha2")]
    pub fn vsha256su1q_u32(a: uint32x4_t, b: uint32x4_t, c: uint32x4_t) -> uint32x4_t {
        let r0 = a.0[0]
            .wrapping_add(b.0[1])
            .wrapping_add(sha256_small1(c.0[2]));
        let r1 = a.0[1]
            .wrapping_add(b.0[2])
            .wrapping_add(sha256_small1(c.0[3]));
        uint32x4_t([
            r0,
            r1,
            a.0[2].wrapping_add(b.0[3]).wrapping_add(sha256_small1(r0)),
            a.0[3].wrapping_add(c.0[0]).wrapping_add(sha256_small1(r1)),
        ])
    }

    fn sha512_big0(x: u64) -> u64 {
        x.rotate_right(28) ^ x.rotate_right(34) ^ x.rotate_right(39)
    }
    fn sha512_big1(x: u64) -> u64 {
        x.rotate_right(14) ^ x.rotate_right(18) ^ x.rotate_right(41)
    }
    fn sha512_small0(x: u64) -> u64 {
        x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7)
    }
    fn sha512_small1(x: u64) -> u64 {
        x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6)
    }

    /// Performs two SHA-512 choose/sigma hash updates.
    ///
    /// # Safety
    /// The caller must enable the SHA3 target feature.
    #[target_feature(enable = "sha3")]
    pub fn vsha512hq_u64(a: uint64x2_t, b: uint64x2_t, c: uint64x2_t) -> uint64x2_t {
        let hi = a.0[1]
            .wrapping_add(sha512_big1(c.0[1]))
            .wrapping_add((c.0[1] & b.0[0]) ^ (!c.0[1] & b.0[1]));
        let e = hi.wrapping_add(c.0[0]);
        let lo = a.0[0]
            .wrapping_add(sha512_big1(e))
            .wrapping_add((e & c.0[1]) ^ (!e & b.0[0]));
        uint64x2_t([lo, hi])
    }

    /// Performs two SHA-512 majority/sigma hash updates.
    ///
    /// # Safety
    /// The caller must enable the SHA3 target feature.
    #[target_feature(enable = "sha3")]
    pub fn vsha512h2q_u64(a: uint64x2_t, b: uint64x2_t, c: uint64x2_t) -> uint64x2_t {
        let hi = a.0[1]
            .wrapping_add(sha512_big0(c.0[0]))
            .wrapping_add((b.0[0] & c.0[1]) ^ (b.0[0] & c.0[0]) ^ (c.0[1] & c.0[0]));
        let lo = a.0[0]
            .wrapping_add(sha512_big0(hi))
            .wrapping_add((hi & c.0[0]) ^ (hi & c.0[1]) ^ (c.0[0] & c.0[1]));
        uint64x2_t([lo, hi])
    }

    /// Performs the first two SHA-512 schedule lane updates.
    ///
    /// # Safety
    /// The caller must enable the SHA3 target feature.
    #[target_feature(enable = "sha3")]
    pub fn vsha512su0q_u64(a: uint64x2_t, b: uint64x2_t) -> uint64x2_t {
        uint64x2_t([
            a.0[0].wrapping_add(sha512_small0(a.0[1])),
            a.0[1].wrapping_add(sha512_small0(b.0[0])),
        ])
    }

    /// Completes two independent SHA-512 schedule lanes.
    ///
    /// # Safety
    /// The caller must enable the SHA3 target feature.
    #[target_feature(enable = "sha3")]
    pub fn vsha512su1q_u64(a: uint64x2_t, b: uint64x2_t, c: uint64x2_t) -> uint64x2_t {
        uint64x2_t([
            a.0[0]
                .wrapping_add(sha512_small1(b.0[0]))
                .wrapping_add(c.0[0]),
            a.0[1]
                .wrapping_add(sha512_small1(b.0[1]))
                .wrapping_add(c.0[1]),
        ])
    }

    /// Eight unsigned byte lanes.
    #[allow(non_camel_case_types)]
    #[repr(C, align(8))]
    #[derive(Clone, Copy)]
    pub struct uint8x8_t(pub(crate) [u8; 8]);

    /// Eight signed byte lanes.
    #[allow(non_camel_case_types)]
    #[repr(C, align(8))]
    #[derive(Clone, Copy)]
    pub struct int8x8_t(pub(crate) [i8; 8]);

    /// One unsigned doubleword lane.
    #[allow(non_camel_case_types)]
    #[repr(C, align(8))]
    #[derive(Clone, Copy)]
    pub struct uint64x1_t(pub(crate) [u64; 1]);

    /// Reads eight bytes without an alignment requirement.
    pub unsafe fn vld1_u8(ptr: *const u8) -> uint8x8_t {
        unsafe { __neon_vld1_u8(ptr) }
    }

    /// Writes eight bytes without an alignment requirement.
    pub unsafe fn vst1_u8(ptr: *mut u8, value: uint8x8_t) {
        unsafe { __neon_vst1_u8(ptr, value) }
    }

    #[target_feature(enable = "neon")]
    pub fn vdup_n_u8(value: u8) -> uint8x8_t {
        __neon_vdup_n_u8(value)
    }

    #[target_feature(enable = "neon")]
    pub fn vceq_u8(a: uint8x8_t, b: uint8x8_t) -> uint8x8_t {
        __neon_vceq_u8(a, b)
    }

    #[target_feature(enable = "neon")]
    pub fn vorr_u8(a: uint8x8_t, b: uint8x8_t) -> uint8x8_t {
        __neon_vorr_u8(a, b)
    }

    #[target_feature(enable = "neon")]
    pub fn vcltz_s8(a: int8x8_t) -> uint8x8_t {
        __neon_vcltz_s8(a)
    }

    #[target_feature(enable = "neon")]
    pub fn vcgez_s8(a: int8x8_t) -> uint8x8_t {
        __neon_vcgez_s8(a)
    }

    #[target_feature(enable = "neon")]
    pub fn vreinterpret_s8_u8(a: uint8x8_t) -> int8x8_t {
        unsafe { *(&a as *const uint8x8_t as *const int8x8_t) }
    }

    #[rustc_intrinsic]
    unsafe fn __neon_vld1_u8(ptr: *const u8) -> uint8x8_t {
        loop {}
    }
    #[rustc_intrinsic]
    unsafe fn __neon_vst1_u8(ptr: *mut u8, value: uint8x8_t) {
        loop {}
    }
    #[rustc_intrinsic]
    fn __neon_vdup_n_u8(value: u8) -> uint8x8_t {
        loop {}
    }
    #[rustc_intrinsic]
    fn __neon_vceq_u8(a: uint8x8_t, b: uint8x8_t) -> uint8x8_t {
        loop {}
    }
    #[rustc_intrinsic]
    fn __neon_vorr_u8(a: uint8x8_t, b: uint8x8_t) -> uint8x8_t {
        loop {}
    }
    #[rustc_intrinsic]
    fn __neon_vcltz_s8(a: int8x8_t) -> uint8x8_t {
        loop {}
    }
    #[rustc_intrinsic]
    fn __neon_vcgez_s8(a: int8x8_t) -> uint8x8_t {
        loop {}
    }

    /// Reads sixteen readable bytes, without an alignment requirement.
    pub unsafe fn vld1q_u8(ptr: *const u8) -> uint8x16_t {
        unsafe { __neon_vld1q_u8(ptr) }
    }

    /// Duplicates a byte into all sixteen lanes.
    #[target_feature(enable = "neon")]
    pub fn vdupq_n_u8(value: u8) -> uint8x16_t {
        __neon_vdupq_n_u8(value)
    }

    /// Compares lanes, producing 255 for equal lanes and zero otherwise.
    #[target_feature(enable = "neon")]
    pub fn vceqq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        __neon_vceqq_u8(a, b)
    }

    /// Bitwise intersection.
    #[target_feature(enable = "neon")]
    pub fn vandq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        __neon_vandq_u8(a, b)
    }

    /// Bitwise union.
    #[target_feature(enable = "neon")]
    pub fn vorrq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        __neon_vorrq_u8(a, b)
    }

    /// Reinterprets all bits as eight halfwords.
    #[target_feature(enable = "neon")]
    pub fn vreinterpretq_u16_u8(a: uint8x16_t) -> uint16x8_t {
        // SAFETY: equal sizes and alignments; all destination bits are valid.
        unsafe { *(&a as *const uint8x16_t as *const uint16x8_t) }
    }

    /// Reinterprets all bits as two doublewords.
    #[target_feature(enable = "neon")]
    pub fn vreinterpretq_u64_u8(a: uint8x16_t) -> uint64x2_t {
        // SAFETY: equal sizes and alignments; all destination bits are valid.
        unsafe { *(&a as *const uint8x16_t as *const uint64x2_t) }
    }

    /// Reinterprets all bits as one doubleword.
    #[target_feature(enable = "neon")]
    pub fn vreinterpret_u64_u8(a: uint8x8_t) -> uint64x1_t {
        // SAFETY: equal sizes and alignments; all destination bits are valid.
        unsafe { *(&a as *const uint8x8_t as *const uint64x1_t) }
    }

    /// Extracts lane 0 or 1. The compiler checks the constant lane.
    #[rustc_legacy_const_generics(1)]
    #[target_feature(enable = "neon")]
    pub fn vgetq_lane_u64<const LANE: i32>(a: uint64x2_t) -> u64 {
        __neon_vgetq_lane_u64::<LANE>(a)
    }

    /// Extracts lane 0. The compiler checks the constant lane.
    #[rustc_legacy_const_generics(1)]
    #[target_feature(enable = "neon")]
    pub fn vget_lane_u64<const LANE: i32>(a: uint64x1_t) -> u64 {
        __neon_vget_lane_u64::<LANE>(a)
    }

    /// Shifts each halfword right by N bits, keeping its low byte.
    /// N must be a compile-time constant in 1..=8.
    ///
    /// # Safety
    /// The caller must run on an AArch64 target with NEON.
    #[rustc_legacy_const_generics(1)]
    #[target_feature(enable = "neon")]
    pub fn vshrn_n_u16<const N: i32>(a: uint16x8_t) -> uint8x8_t {
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
    #[target_feature(enable = "neon")]
    pub fn vpmaxq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
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
