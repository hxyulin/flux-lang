//! Target register types for Rust inline assembly.
use crate::{Signatures, Ty, TyKind};
use flux_hir::{AsmRegister, ItemTree};

/// Whether `ty` is one of the actual embedded AArch64 SIMD types.
pub fn is_vector(items: &ItemTree, ty: Ty) -> bool {
    let TyKind::Adt(adt, _) = ty.kind() else {
        return false;
    };
    let data = &items.structs[adt.strukt];
    items.is_core(data.loc.module)
        && items.modules[data.loc.module].name.as_str() == "aarch64"
        && matches!(
            data.name.as_str(),
            "int8x8_t"
                | "int8x16_t"
                | "uint8x8_t"
                | "uint8x16_t"
                | "int16x4_t"
                | "int16x8_t"
                | "uint16x4_t"
                | "uint16x8_t"
                | "int32x2_t"
                | "int32x4_t"
                | "uint32x2_t"
                | "uint32x4_t"
                | "int64x1_t"
                | "int64x2_t"
                | "uint64x1_t"
                | "uint64x2_t"
        )
}

pub fn supports(items: &ItemTree, sigs: &Signatures, ty: Ty, register: AsmRegister) -> bool {
    match register.class() {
        AsmRegister::Reg => match ty.kind() {
            TyKind::Int(i) => i.bits() <= 64,
            TyKind::Float(_) => true,
            TyKind::FnPtr(_) => true,
            TyKind::RawPtr(_, p) => !sigs.is_unsized(p),
            _ => false,
        },
        AsmRegister::Vreg => {
            matches!(ty.kind(), TyKind::Float(_))
                || ty.as_int().is_some_and(|i| i.bits() <= 64)
                || is_vector(items, ty)
        }
        _ => unreachable!(),
    }
}
