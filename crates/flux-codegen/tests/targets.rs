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
