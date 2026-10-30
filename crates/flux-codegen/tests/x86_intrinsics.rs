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
    for vector in ["__m128i", "__m256i"] {
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
