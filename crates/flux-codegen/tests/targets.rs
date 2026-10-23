use flux_codegen::{OptimizationTier, Target};

#[test]
fn supported_object_formats_and_unimplemented_linux_target() {
    let (_, _, hir) = flux_hir::lower_text("main.rs", "fn main() { println!(\"{}\", 42); }");
    assert!(hir.diagnostics.is_empty());
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty());
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty());
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for tier in [
        OptimizationTier::None,
        OptimizationTier::Fast,
        OptimizationTier::Basic,
        OptimizationTier::Full,
    ] {
        for (target, magic) in [
            (Target::Aarch64AppleDarwin, [0xcf, 0xfa, 0xed, 0xfe]),
            (Target::X86_64LinuxGnu, *b"\x7fELF"),
        ] {
            let object = flux_codegen::compile_object(&mut module.clone(), target, tier).unwrap();
            assert_eq!(object[..4], magic, "{target:?} {tier:?}");
        }
    }
    assert_eq!(
        flux_codegen::compile_object(
            &mut module.clone(),
            Target::Aarch64LinuxGnu,
            OptimizationTier::None,
        )
        .unwrap_err(),
        "code generation for AArch64 Linux is not supported yet"
    );
    assert_eq!(
        flux_codegen::link_executable(
            &[],
            Target::Aarch64LinuxGnu,
            std::path::Path::new("unused"),
        )
        .unwrap_err(),
        "linking for AArch64 Linux is not supported yet"
    );
}

#[test]
fn opt_in_timings_preserve_objects_and_partial_verification_failure() {
    let (_, _, hir) = flux_hir::lower_text("main.rs", "fn main() { println!(\"timed\"); }");
    let typed = flux_mir::check_file(&hir);
    let mir = flux_mir::build_file(&hir, &typed);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for tier in [OptimizationTier::None, OptimizationTier::Full] {
        let ordinary =
            flux_codegen::compile_object(&mut module.clone(), Target::Aarch64AppleDarwin, tier)
                .unwrap();
        let (timed, scopes) = flux_codegen::compile_object_with_timings(
            &mut module.clone(),
            Target::Aarch64AppleDarwin,
            tier,
        );
        assert_eq!(timed.unwrap(), ordinary);
        assert!(scopes.verify.is_some());
        assert!(scopes.optimize.is_some());
        assert!(scopes.codegen.is_some());
        assert!(scopes.assemble.is_some());
    }
    // An unterminated block must fail before optimization or native codegen.
    let mut invalid = keel_ir::Module::default();
    let mut function = keel_ir::Function::new("invalid", keel_ir::Signature::new(&[], &[]));
    function.create_block();
    invalid.functions.push(function);
    let ordinary = flux_codegen::compile_object(
        &mut invalid.clone(),
        Target::Aarch64AppleDarwin,
        OptimizationTier::None,
    )
    .unwrap_err();
    let (result, scopes) = flux_codegen::compile_object_with_timings(
        &mut invalid,
        Target::Aarch64AppleDarwin,
        OptimizationTier::None,
    );
    assert_eq!(result.unwrap_err(), ordinary);
    assert!(scopes.verify.is_some());
    assert!(scopes.optimize.is_none());
    assert!(scopes.codegen.is_none());
    assert!(scopes.assemble.is_none());
}
