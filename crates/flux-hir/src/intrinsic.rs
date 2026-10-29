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
    SizedDispatch => "__sized_dispatch",
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
}

impl Intrinsic {
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
