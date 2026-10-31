//! Vector operations retain function-local ISA permissions through lowering.
#![cfg(target_arch = "x86_64")]

#[test]
fn packed_operations_use_native_instructions_with_local_features() {
    let source = r#"
use core::arch::x86_64::*;
#[target_feature(enable="avx2")]
fn probe(p:*const __m256i)->i32 {
    unsafe {
        let a=_mm256_loadu_si256(p);
        let b=_mm256_add_epi64(a,a);
        _mm256_movemask_epi8(_mm256_cmpeq_epi8(a,b))
    }
}
fn main(){let values=[0u8;32];println!("{}",unsafe{probe(values.as_ptr().cast())});}
"#;
    let (_, _, hir) = flux_hir::lower_text("main.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let probe = module
        .functions
        .iter()
        .find(|f| f.name.contains("probe"))
        .unwrap();
    assert!(
        probe
            .attrs
            .target_features
            .resolve(Default::default())
            .unwrap()
            .avx2
    );
    let main = module
        .functions
        .iter()
        .find(|f| f.name.contains("main"))
        .unwrap();
    assert!(
        main.attrs.target_features.is_empty(),
        "main keeps the baseline ISA"
    );
    let assembly = keel_codegen::compile(&module, flux_codegen::Target::X86_64LinuxGnu).unwrap();
    for mnemonic in ["vpaddq", "vpcmpeqb", "vpmovmskb", "ymm"] {
        assert!(
            assembly.contains(mnemonic),
            "missing {mnemonic}:\n{assembly}"
        );
    }
}

#[test]
fn vectors_are_rejected_at_c_abi_boundaries() {
    for vector in ["__m128i", "__m256i", "__m512i"] {
        let source = format!(
            "use core::arch::x86_64::*; unsafe extern \"C\" {{ fn take(value:{vector}); fn make()->{vector}; }} fn main(){{}}"
        );
        let (_, _, hir) = flux_hir::lower_text("main.rs", &source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let errors = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_err();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(
            errors
                .iter()
                .all(|d| d.message.contains("C ABI is not supported")),
            "{errors:?}"
        );
    }
}

#[test]
fn crypto_operations_use_zmm_and_sha_instructions() {
    let source = r#"
use core::arch::x86_64::*;
#[target_feature(enable="avx512f,vpclmulqdq")]
unsafe fn fold(p:*const __m512i,q:*const __m512i)->__m512i {
    unsafe {
        let a=_mm512_loadu_si512(p);
        let b=_mm512_loadu_si512(q);
        _mm512_xor_si512(a,_mm512_clmulepi64_epi128::<17>(a,b))
    }
}
#[target_feature(enable="sha")]
fn sha(a:__m128i,b:__m128i)->__m128i {
    _mm_sha256rnds2_epu32(a,b,_mm_sha256msg2_epu32(a,_mm_sha256msg1_epu32(a,b)))
}
fn main(){unsafe{let a=_mm512_setzero_si512();let v=fold(&a,&a);let low=_mm512_castsi512_si128(v);let _=sha(low,low);}}
"#;
    let (_, _, hir) = flux_hir::lower_text("main.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let assembly = keel_codegen::compile(&module, flux_codegen::Target::X86_64LinuxGnu).unwrap();
    for mnemonic in [
        "zmm",
        "vpclmul",
        "vpxor",
        "sha256msg1",
        "sha256msg2",
        "sha256rnds2",
    ] {
        assert!(
            assembly.contains(mnemonic),
            "missing {mnemonic}:\n{assembly}"
        );
    }
}
