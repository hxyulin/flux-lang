//! Assembly remains opaque even when every register output is unused.
#[test]
fn tied_asm_effects_survive_optimization() {
    let source = r#"
        use core::arch::asm;
        fn main() {
            let mut x=7u64;
            unsafe { asm!("/*{0}*/",inout(reg)x); }
            unsafe { asm!("nop"); }
        }
    "#;
    let (_, _, hir) = flux_hir::lower_text("main.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut module = module.clone();
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let mut count = 0;
        for function in &module.functions {
            for (_, instruction) in function.layout_insts() {
                if let keel_ir::InstData::InlineAsm { asm, args } = function.inst_data(instruction)
                {
                    count += 1;
                    assert!(asm.clobbers.contains(&keel_ir::AsmClobber::Memory));
                    assert!(asm.clobbers.contains(&keel_ir::AsmClobber::Flags));
                    assert_eq!(
                        function.inst_data(instruction).memory_effects(),
                        keel_ir::MemoryEffects::Barrier
                    );
                    if !asm.outputs.is_empty() {
                        assert_eq!(
                            asm.outputs,
                            [(keel_ir::AsmOperand::Reg, keel_ir::Type::I64)]
                        );
                        assert_eq!(asm.inputs, [keel_ir::AsmOperand::Tied(0)]);
                        assert_eq!(args.len(), 1);
                    }
                }
            }
        }
        assert_eq!(count, 2, "{tier:?}");
        keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
    }
}
