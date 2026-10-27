//! Legacy closure captures and inactive enum payloads preserve mutable access.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn mutable_capture_regions_match_rustc() {
    support::oracle::compare_fixture("mutable_capture_regions");
}
#[test]
fn legacy_mutable_capture_regions_match_rustc() {
    use std::process::Command;
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = repo.join("tests/run/mutable_capture_regions.rs");
    let root = repo.join(format!(
        "out/legacy-mutable-captures/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let reference = root.join("rustc");
    let status = Command::new("rustc")
        .args(["--edition=2018", "-Cpanic=abort"])
        .arg(&path)
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let expected = Command::new(reference).output().unwrap();
    assert!(expected.status.success());
    let mut db = flux_hir::SourceDb::new();
    let file = db
        .load_with_edition(&path, flux_syntax::Edition::E2018)
        .unwrap();
    let hir = flux_hir::lower(&mut db, file, flux_intern::Symbol::intern("main"));
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    for optimized in [false, true] {
        let target = flux_codegen::Target::Aarch64AppleDarwin;
        let tier = if optimized {
            flux_codegen::OptimizationTier::Full
        } else {
            flux_codegen::OptimizationTier::None
        };
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
        let executable = root.join(format!("flux-{optimized}"));
        flux_codegen::link_executable(&object, target, &executable).unwrap();
        let actual = Command::new(executable).output().unwrap();
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}
