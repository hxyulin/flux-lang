use keel_ir::InstData;
fn accesses(
    module: &keel_ir::Module,
    function: &keel_ir::Function,
    depth: usize,
) -> (usize, usize) {
    assert!(depth < 16, "unexpected recursive volatile probe");
    let mut count = (0, 0);
    for (_, inst) in function.layout_insts() {
        match function.inst_data(inst) {
            InstData::Load { flags, .. } if flags.volatile => {
                assert!(!flags.notrap);
                count.0 += 1;
            }
            InstData::Store { flags, .. } if flags.volatile => {
                assert!(!flags.notrap);
                count.1 += 1;
            }
            InstData::Call { func, .. } => {
                let name = &function.callee(*func).name;
                if let Some(callee) = module.functions.iter().find(|f| &f.name == name) {
                    let nested = accesses(module, callee, depth + 1);
                    count.0 += nested.0;
                    count.1 += nested.1;
                }
            }
            _ => {}
        }
    }
    count
}
#[test]
fn volatile_qualifiers_and_accesses_survive_optimization() {
    let (_, _, hir) = flux_hir::lower_text(
        "volatile.rs",
        r#"
        #[inline(never)]fn volatile_accesses(pointer:*mut u32)->u32{unsafe{
            core::ptr::write_volatile(pointer,1);let _=core::ptr::read_volatile(pointer);
            core::ptr::write_volatile(pointer,2);core::ptr::read_volatile(pointer)
        }}
        fn main(){let mut value=0;assert_eq!(volatile_accesses(&mut value),2);}
    "#,
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let original = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut module = original.clone();
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let function = module
            .functions
            .iter()
            .find(|function| function.name.contains("volatile_accesses"))
            .expect("noinline probe");
        assert_eq!(
            accesses(&module, function, 0),
            (2, 2),
            "volatile accesses removed/duplicated at {tier:?}"
        );
        assert!(module.to_string().contains("volatile"));
        flux_codegen::compile_object(
            &mut module,
            flux_codegen::Target::Aarch64AppleDarwin,
            keel_opt::OptimizationTier::None,
        )
        .unwrap();
    }
}
