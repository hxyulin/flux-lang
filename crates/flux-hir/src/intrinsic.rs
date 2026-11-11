//! Identities of compiler operations declared by the embedded libraries.
//! Resolve names at collection; later stages retain their own semantics.

// Keep the identity and spelling together. Adding an operation is one entry;
// it does not duplicate a string-to-enum and enum-to-string registry.
macro_rules! intrinsics {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// A compiler operation from a trusted embedded declaration.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Intrinsic { $(#[doc = $name] $variant,)* }
        impl Intrinsic {
    /// An integer's leading or trailing zeros, set bits, or bytes in
    /// reverse order: one machine instruction each.
    pub const fn is_bit_count(self) -> bool {
        matches!(self, Self::Ctlz | Self::Cttz | Self::Ctpop | Self::Bswap)
    }

            pub(crate) fn from_name(name: &str) -> Option<Self> {
                Some(match name { $($name => Self::$variant,)* _ => return None })
            }
            /// The canonical spelling in the embedded library.
            pub const fn name(self) -> &'static str {
                match self { $(Self::$variant => $name,)* }
            }
        }
    };
}

intrinsics! {
    X86MmAndSi128 => "__x86_mm_and_si128",
    X86MmOrSi128 => "__x86_mm_or_si128",
    X86MmXorSi128 => "__x86_mm_xor_si128",
    X86MmAddEpi32 => "__x86_mm_add_epi32",
    X86MmAddEpi64 => "__x86_mm_add_epi64",
    X86MmSubEpi16 => "__x86_mm_sub_epi16",
    X86MmMulloEpi16 => "__x86_mm_mullo_epi16",
    X86MmCmpeqEpi8 => "__x86_mm_cmpeq_epi8",
    X86MmCmpgtEpi8 => "__x86_mm_cmpgt_epi8",
    X86MmLoaduSi128 => "__x86_mm_loadu_si128",
    X86MmLoadSi128 => "__x86_mm_load_si128",
    X86MmMovemaskEpi8 => "__x86_mm_movemask_epi8",
    X86MmShuffleEpi8 => "__x86_mm_shuffle_epi8",
    X86MmSlliEpi16 => "__x86_mm_slli_epi16",
    X86MmSrliEpi16 => "__x86_mm_srli_epi16",
    X86MmSlliEpi32 => "__x86_mm_slli_epi32",
    X86MmSrliEpi32 => "__x86_mm_srli_epi32",
    X86MmSlliEpi64 => "__x86_mm_slli_epi64",
    X86MmSrliEpi64 => "__x86_mm_srli_epi64",
    X86Mm256AndSi256 => "__x86_mm256_and_si256",
    X86Mm256OrSi256 => "__x86_mm256_or_si256",
    X86Mm256XorSi256 => "__x86_mm256_xor_si256",
    X86Mm256AddEpi32 => "__x86_mm256_add_epi32",
    X86Mm256AddEpi64 => "__x86_mm256_add_epi64",
    X86Mm256SubEpi16 => "__x86_mm256_sub_epi16",
    X86Mm256MulloEpi16 => "__x86_mm256_mullo_epi16",
    X86Mm256CmpeqEpi8 => "__x86_mm256_cmpeq_epi8",
    X86Mm256CmpgtEpi8 => "__x86_mm256_cmpgt_epi8",
    X86Mm256LoaduSi256 => "__x86_mm256_loadu_si256",
    X86Mm256LoadSi256 => "__x86_mm256_load_si256",
    X86Mm256MovemaskEpi8 => "__x86_mm256_movemask_epi8",
    X86Mm256ShuffleEpi8 => "__x86_mm256_shuffle_epi8",
    X86Mm256SlliEpi16 => "__x86_mm256_slli_epi16",
    X86Mm256SrliEpi16 => "__x86_mm256_srli_epi16",
    X86Mm256SlliEpi32 => "__x86_mm256_slli_epi32",
    X86Mm256SrliEpi32 => "__x86_mm256_srli_epi32",
    X86Mm256SlliEpi64 => "__x86_mm256_slli_epi64",
    X86Mm256SrliEpi64 => "__x86_mm256_srli_epi64",
    X86MmMulhiEpu16 => "__x86_mm_mulhi_epu16",
    X86MmMulEpu32 => "__x86_mm_mul_epu32",
    X86Mm512XorSi512 => "__x86_mm512_xor_si512",
    X86Mm512LoaduSi512 => "__x86_mm512_loadu_si512",
    X86MmClmulepi64Si128 => "__x86_mm_clmulepi64_si128",
    X86Mm256Clmulepi64Epi128 => "__x86_mm256_clmulepi64_epi128",
    X86Mm512Clmulepi64Epi128 => "__x86_mm512_clmulepi64_epi128",
    X86MmSha256msg1Epu32 => "__x86_mm_sha256msg1_epu32",
    X86MmSha256msg2Epu32 => "__x86_mm_sha256msg2_epu32",
    X86MmSha256rnds2Epu32 => "__x86_mm_sha256rnds2_epu32",
    X86Cpuid => "__x86_cpuid",
    X86Xgetbv => "__x86_xgetbv",
    X86MmShuffleEpi32 => "__x86_mm_shuffle_epi32",
    X86MmAlignrEpi8 => "__x86_mm_alignr_epi8",
    X86Mm256AlignrEpi8 => "__x86_mm256_alignr_epi8",
    X86MmBlendEpi16 => "__x86_mm_blend_epi16",
    X86MmAddEpi8 => "__x86_mm_add_epi8",
    X86MmCmpeqEpi16 => "__x86_mm_cmpeq_epi16",
    X86MmCmpeqEpi32 => "__x86_mm_cmpeq_epi32",
    X86MmCmpgtEpi16 => "__x86_mm_cmpgt_epi16",
    X86MmCmpgtEpi32 => "__x86_mm_cmpgt_epi32",
    X86MmPacksEpi16 => "__x86_mm_packs_epi16",
    X86MmPacksEpi32 => "__x86_mm_packs_epi32",
    X86MmSrliSi128 => "__x86_mm_srli_si128",
    X86MmExtractEpi32 => "__x86_mm_extract_epi32",
    X86MmPrefetch => "__x86_mm_prefetch",
    PtrWithAddr => "ptr_with_addr",
    PtrMetadataWord => "ptr_metadata_word",
    AtomicU16CasRelaxed => "__atomic_u16_cas_relaxed",
    AtomicU16CasAcquire => "__atomic_u16_cas_acquire",
    AtomicU16CasRelease => "__atomic_u16_cas_release",
    AtomicU16CasAcqrel => "__atomic_u16_cas_acqrel",
    AtomicU16CasSeqcst => "__atomic_u16_cas_seqcst",
    AtomicU32CasRelaxed => "__atomic_u32_cas_relaxed",
    AtomicU32CasAcquire => "__atomic_u32_cas_acquire",
    AtomicU32CasRelease => "__atomic_u32_cas_release",
    AtomicU32CasAcqrel => "__atomic_u32_cas_acqrel",
    AtomicU32CasSeqcst => "__atomic_u32_cas_seqcst",
    AtomicPtrCasRelaxed => "__atomic_ptr_cas_relaxed",
    AtomicPtrCasAcquire => "__atomic_ptr_cas_acquire",
    AtomicPtrCasRelease => "__atomic_ptr_cas_release",
    AtomicPtrCasAcqrel => "__atomic_ptr_cas_acqrel",
    AtomicPtrCasSeqcst => "__atomic_ptr_cas_seqcst",
    CoroutineNew => "__coroutine_new",
    CoroutineAwait => "__coroutine_await",
    CoroutinePoll => "__coroutine_poll",
    CoroutineDrop => "__coroutine_drop",
    SizedDispatch => "__sized_dispatch",
    BoxCallOnce => "__box_call_once",
    PanicUnwindEnabled => "__panic_unwind_enabled",
    PanicCatch => "__panic_catch",
    NeonVld1U8 => "__neon_vld1_u8",
    NeonVst1U8 => "__neon_vst1_u8",
    NeonVdupNU8 => "__neon_vdup_n_u8",
    NeonVceqU8 => "__neon_vceq_u8",
    NeonVorrU8 => "__neon_vorr_u8",
    NeonVcltzS8 => "__neon_vcltz_s8",
    NeonVcgezS8 => "__neon_vcgez_s8",
    NeonVshrnNU16 => "__neon_vshrn_n_u16",
    NeonVpmaxqU8 => "__neon_vpmaxq_u8",
    NeonVld1qU8 => "__neon_vld1q_u8",
    NeonVdupqNU8 => "__neon_vdupq_n_u8",
    NeonVceqqU8 => "__neon_vceqq_u8",
    NeonVandqU8 => "__neon_vandq_u8",
    NeonVorrqU8 => "__neon_vorrq_u8",
    NeonVgetqLaneU64 => "__neon_vgetq_lane_u64",
    NeonVgetLaneU64 => "__neon_vget_lane_u64",
    UnreachableUnchecked => "unreachable_unchecked",
    Read => "read",
    Write => "write",
    Uninit => "uninit",
    IsCopy => "is_copy",
    SizeOf => "size_of",
    AlignOf => "align_of",
    SizeOfVal => "size_of_val",
    AlignOfVal => "align_of_val",
    DropInPlace => "drop_in_place",
    NeedsDrop => "needs_drop",
    DiscriminantValue => "discriminant_value",
    DiscriminantIsSigned => "discriminant_is_signed",
    Subslice => "subslice",
    SubsliceMut => "subslice_mut",
    SliceFromRawParts => "slice_from_raw_parts",
    RawSliceLen => "raw_slice_len",
    PtrAdd => "ptr_add",
    PtrSub => "ptr_sub",
    PtrOffset => "ptr_offset",
    SliceFromRawPartsMut => "slice_from_raw_parts_mut",
    RawSliceFromParts => "raw_slice_from_parts",
    RawSliceFromPartsMut => "raw_slice_from_parts_mut",
    CallerLocation => "caller_location",
    ReadUnaligned => "read_unaligned",
    ReadVolatile => "read_volatile",
    WriteVolatile => "write_volatile",
    TypeName => "type_name",
    TypeId => "type_id",
    AtomicUsizeLoadRelaxed => "__atomic_usize_load_relaxed",
    AtomicUsizeLoadAcquire => "__atomic_usize_load_acquire",
    AtomicUsizeLoadSeqcst => "__atomic_usize_load_seqcst",
    AtomicUsizeStoreRelaxed => "__atomic_usize_store_relaxed",
    AtomicUsizeStoreRelease => "__atomic_usize_store_release",
    AtomicUsizeStoreSeqcst => "__atomic_usize_store_seqcst",
    AtomicUsizeAddRelaxed => "__atomic_usize_add_relaxed",
    AtomicUsizeAddAcquire => "__atomic_usize_add_acquire",
    AtomicUsizeAddRelease => "__atomic_usize_add_release",
    AtomicUsizeAddAcqrel => "__atomic_usize_add_acqrel",
    AtomicUsizeAddSeqcst => "__atomic_usize_add_seqcst",
    AtomicUsizeSubRelaxed => "__atomic_usize_sub_relaxed",
    AtomicUsizeSubAcquire => "__atomic_usize_sub_acquire",
    AtomicUsizeSubRelease => "__atomic_usize_sub_release",
    AtomicUsizeSubAcqrel => "__atomic_usize_sub_acqrel",
    AtomicUsizeSubSeqcst => "__atomic_usize_sub_seqcst",
    AtomicUsizeCasRelaxed => "__atomic_usize_cas_relaxed",
    AtomicUsizeCasAcquire => "__atomic_usize_cas_acquire",
    AtomicUsizeCasRelease => "__atomic_usize_cas_release",
    AtomicUsizeCasAcqrel => "__atomic_usize_cas_acqrel",
    AtomicUsizeCasSeqcst => "__atomic_usize_cas_seqcst",
    AtomicUsizeFenceAcquire => "__atomic_usize_fence_acquire",
    AtomicUsizeFenceRelease => "__atomic_usize_fence_release",
    AtomicUsizeFenceAcqrel => "__atomic_usize_fence_acqrel",
    AtomicUsizeFenceSeqcst => "__atomic_usize_fence_seqcst",
    AtomicU8LoadRelaxed => "__atomic_u8_load_relaxed",
    AtomicU8LoadAcquire => "__atomic_u8_load_acquire",
    AtomicU8LoadSeqcst => "__atomic_u8_load_seqcst",
    AtomicU8StoreRelaxed => "__atomic_u8_store_relaxed",
    AtomicU8StoreRelease => "__atomic_u8_store_release",
    AtomicU8StoreSeqcst => "__atomic_u8_store_seqcst",
    AtomicU8CasRelaxed => "__atomic_u8_cas_relaxed",
    AtomicU8CasAcquire => "__atomic_u8_cas_acquire",
    AtomicU8CasRelease => "__atomic_u8_cas_release",
    AtomicU8CasAcqrel => "__atomic_u8_cas_acqrel",
    AtomicU8CasSeqcst => "__atomic_u8_cas_seqcst",
    AtomicU16LoadRelaxed => "__atomic_u16_load_relaxed",
    AtomicU16LoadAcquire => "__atomic_u16_load_acquire",
    AtomicU16LoadSeqcst => "__atomic_u16_load_seqcst",
    AtomicU16StoreRelaxed => "__atomic_u16_store_relaxed",
    AtomicU16StoreRelease => "__atomic_u16_store_release",
    AtomicU16StoreSeqcst => "__atomic_u16_store_seqcst",
    AtomicU32LoadRelaxed => "__atomic_u32_load_relaxed",
    AtomicU32LoadAcquire => "__atomic_u32_load_acquire",
    AtomicU32LoadSeqcst => "__atomic_u32_load_seqcst",
    AtomicU32StoreRelaxed => "__atomic_u32_store_relaxed",
    AtomicU32StoreRelease => "__atomic_u32_store_release",
    AtomicU32StoreSeqcst => "__atomic_u32_store_seqcst",
    AtomicU32AddRelaxed => "__atomic_u32_add_relaxed",
    AtomicU32AddAcquire => "__atomic_u32_add_acquire",
    AtomicU32AddRelease => "__atomic_u32_add_release",
    AtomicU32AddAcqrel => "__atomic_u32_add_acqrel",
    AtomicU32AddSeqcst => "__atomic_u32_add_seqcst",
    AtomicPtrLoadRelaxed => "__atomic_ptr_load_relaxed",
    AtomicPtrLoadAcquire => "__atomic_ptr_load_acquire",
    AtomicPtrLoadSeqcst => "__atomic_ptr_load_seqcst",
    AtomicPtrStoreRelaxed => "__atomic_ptr_store_relaxed",
    AtomicPtrStoreRelease => "__atomic_ptr_store_release",
    AtomicPtrStoreSeqcst => "__atomic_ptr_store_seqcst",
    Transmute => "transmute",
    Fmaf32 => "fmaf32",
    Fmaf64 => "fmaf64",
    Ctlz => "ctlz",
    Cttz => "cttz",
    Ctpop => "ctpop",
    Bswap => "bswap",
}

impl Intrinsic {
    pub const fn is_x86(self) -> bool {
        matches!(
            self,
            Self::X86Mm512XorSi512
                | Self::X86Mm512LoaduSi512
                | Self::X86MmClmulepi64Si128
                | Self::X86Mm256Clmulepi64Epi128
                | Self::X86Mm512Clmulepi64Epi128
                | Self::X86MmSha256msg1Epu32
                | Self::X86MmSha256msg2Epu32
                | Self::X86MmSha256rnds2Epu32
                | Self::X86MmShuffleEpi32
                | Self::X86MmAlignrEpi8
                | Self::X86Mm256AlignrEpi8
                | Self::X86MmBlendEpi16
                | Self::X86MmAddEpi8
                | Self::X86MmCmpeqEpi16
                | Self::X86MmCmpeqEpi32
                | Self::X86MmCmpgtEpi16
                | Self::X86MmCmpgtEpi32
                | Self::X86MmPacksEpi16
                | Self::X86MmPacksEpi32
                | Self::X86MmSrliSi128
                | Self::X86MmExtractEpi32
                | Self::X86MmPrefetch
                | Self::X86MmAndSi128
                | Self::X86MmOrSi128
                | Self::X86MmXorSi128
                | Self::X86MmAddEpi32
                | Self::X86MmAddEpi64
                | Self::X86MmSubEpi16
                | Self::X86MmMulloEpi16
                | Self::X86MmCmpeqEpi8
                | Self::X86MmCmpgtEpi8
                | Self::X86MmLoaduSi128
                | Self::X86MmLoadSi128
                | Self::X86MmMovemaskEpi8
                | Self::X86MmShuffleEpi8
                | Self::X86MmSlliEpi16
                | Self::X86MmSrliEpi16
                | Self::X86MmSlliEpi32
                | Self::X86MmSrliEpi32
                | Self::X86MmSlliEpi64
                | Self::X86MmSrliEpi64
                | Self::X86Mm256AndSi256
                | Self::X86Mm256OrSi256
                | Self::X86Mm256XorSi256
                | Self::X86Mm256AddEpi32
                | Self::X86Mm256AddEpi64
                | Self::X86Mm256SubEpi16
                | Self::X86Mm256MulloEpi16
                | Self::X86Mm256CmpeqEpi8
                | Self::X86Mm256CmpgtEpi8
                | Self::X86Mm256LoaduSi256
                | Self::X86Mm256LoadSi256
                | Self::X86Mm256MovemaskEpi8
                | Self::X86Mm256ShuffleEpi8
                | Self::X86Mm256SlliEpi16
                | Self::X86Mm256SrliEpi16
                | Self::X86Mm256SlliEpi32
                | Self::X86Mm256SrliEpi32
                | Self::X86Mm256SlliEpi64
                | Self::X86Mm256SrliEpi64
                | Self::X86MmMulhiEpu16
                | Self::X86MmMulEpu32
                | Self::X86Cpuid
                | Self::X86Xgetbv
        )
    }

    pub const fn is_neon(self) -> bool {
        matches!(
            self,
            Self::NeonVld1U8
                | Self::NeonVst1U8
                | Self::NeonVdupNU8
                | Self::NeonVceqU8
                | Self::NeonVorrU8
                | Self::NeonVcltzS8
                | Self::NeonVcgezS8
                | Self::NeonVshrnNU16
                | Self::NeonVpmaxqU8
                | Self::NeonVld1qU8
                | Self::NeonVdupqNU8
                | Self::NeonVceqqU8
                | Self::NeonVandqU8
                | Self::NeonVorrqU8
                | Self::NeonVgetqLaneU64
                | Self::NeonVgetLaneU64
        )
    }
    pub const fn is_atomic(self) -> bool {
        matches!(
            self,
            Self::AtomicUsizeLoadRelaxed
                | Self::AtomicUsizeLoadAcquire
                | Self::AtomicUsizeLoadSeqcst
                | Self::AtomicUsizeStoreRelaxed
                | Self::AtomicUsizeStoreRelease
                | Self::AtomicUsizeStoreSeqcst
                | Self::AtomicUsizeAddRelaxed
                | Self::AtomicUsizeAddAcquire
                | Self::AtomicUsizeAddRelease
                | Self::AtomicUsizeAddAcqrel
                | Self::AtomicUsizeAddSeqcst
                | Self::AtomicUsizeSubRelaxed
                | Self::AtomicUsizeSubAcquire
                | Self::AtomicUsizeSubRelease
                | Self::AtomicUsizeSubAcqrel
                | Self::AtomicUsizeSubSeqcst
                | Self::AtomicUsizeCasRelaxed
                | Self::AtomicUsizeCasAcquire
                | Self::AtomicUsizeCasRelease
                | Self::AtomicUsizeCasAcqrel
                | Self::AtomicUsizeCasSeqcst
                | Self::AtomicUsizeFenceAcquire
                | Self::AtomicUsizeFenceRelease
                | Self::AtomicUsizeFenceAcqrel
                | Self::AtomicUsizeFenceSeqcst
                | Self::AtomicU8LoadRelaxed
                | Self::AtomicU8LoadAcquire
                | Self::AtomicU8LoadSeqcst
                | Self::AtomicU8StoreRelaxed
                | Self::AtomicU8StoreRelease
                | Self::AtomicU8StoreSeqcst
                | Self::AtomicU8CasRelaxed
                | Self::AtomicU8CasAcquire
                | Self::AtomicU8CasRelease
                | Self::AtomicU8CasAcqrel
                | Self::AtomicU8CasSeqcst
                | Self::AtomicU16LoadRelaxed
                | Self::AtomicU16LoadAcquire
                | Self::AtomicU16LoadSeqcst
                | Self::AtomicU16StoreRelaxed
                | Self::AtomicU16StoreRelease
                | Self::AtomicU16StoreSeqcst
                | Self::AtomicU32LoadRelaxed
                | Self::AtomicU32LoadAcquire
                | Self::AtomicU32LoadSeqcst
                | Self::AtomicU32StoreRelaxed
                | Self::AtomicU32StoreRelease
                | Self::AtomicU32StoreSeqcst
                | Self::AtomicU32AddRelaxed
                | Self::AtomicU32AddAcquire
                | Self::AtomicU32AddRelease
                | Self::AtomicU32AddAcqrel
                | Self::AtomicU32AddSeqcst
                | Self::AtomicPtrLoadRelaxed
                | Self::AtomicPtrLoadAcquire
                | Self::AtomicPtrLoadSeqcst
                | Self::AtomicPtrStoreRelaxed
                | Self::AtomicPtrStoreRelease
                | Self::AtomicPtrStoreSeqcst
        )
    }
}

impl Intrinsic {
    /// Coroutine operations have generated function bodies rather than local
    /// expression expansions.
    pub const fn is_coroutine(self) -> bool {
        matches!(
            self,
            Self::CoroutineNew | Self::CoroutineAwait | Self::CoroutinePoll | Self::CoroutineDrop
        )
    }
}
