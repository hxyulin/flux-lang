#[test]
fn transparent_narrow_scalars_preserve_c_extension() {
    let (_, _, hir) = flux_hir::lower_text(
        "transparent-c.rs",
        r#"
    #[repr(transparent)]struct Signed(std::marker::PhantomData<()>,i8);
    #[repr(transparent)]struct Unsigned(u8);
    #[unsafe(no_mangle)]pub extern "C" fn transparent_signed(a:Signed,b:Unsigned)->Signed{if b.0==0{Signed(std::marker::PhantomData,-1)}else{a}}
    fn main(){let _=transparent_signed(Signed(std::marker::PhantomData,-91),Unsigned(250));}
    "#,
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let function = module
        .functions
        .iter()
        .find(|f| f.name == "transparent_signed")
        .unwrap();
    assert_eq!(function.sig.params.len(), 2);
    assert_eq!(function.sig.params[0].ty, keel_ir::Type::I8);
    assert_eq!(function.sig.params[0].ext, keel_ir::ArgExt::Sext);
    assert_eq!(function.sig.params[1].ty, keel_ir::Type::I8);
    assert_eq!(function.sig.params[1].ext, keel_ir::ArgExt::Uext);
    assert_eq!(function.sig.returns.len(), 1);
    assert_eq!(function.sig.returns[0].ext, keel_ir::ArgExt::Sext);
}

#[test]
fn transparent_wrappers_preserve_payload_admission() {
    for declaration in [
        "enum Payload { A, B }",
        "enum Payload { None, Some(extern \"C\" fn()) }",
    ] {
        for payload in ["Payload", "Inner"] {
            let source = format!(
                "{declaration} #[repr(transparent)] struct Inner(Payload); \
                 #[repr(transparent)] struct Wrapper({payload}); \
                 unsafe extern \"C\" {{ fn take(value: Wrapper); fn make() -> Wrapper; }} \
                 fn main() {{}}"
            );
            let (_, _, hir) = flux_hir::lower_text("transparent-unsupported.rs", &source);
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
                    .all(|error| error.message.contains("C ABI is not supported")),
                "{errors:?}"
            );
        }
    }
}
