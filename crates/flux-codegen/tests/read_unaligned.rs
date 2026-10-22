fn front_end(text: &str) -> (flux_hir::Hir, flux_ty::TypedFile, flux_mir::FileMir) {
    let (_, _, hir) = flux_hir::lower_text("main.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    (hir, typed, mir)
}

#[test]
fn scalar_load_has_no_alignment_or_fault_freedom_promise() {
    for (rust_ty, keel_ty) in [
        ("u8", keel_ir::Type::I8),
        ("u16", keel_ir::Type::I16),
        ("u32", keel_ir::Type::I32),
        ("u64", keel_ir::Type::I64),
        ("usize", keel_ir::Type::I64),
    ] {
        let text = format!(
            "unsafe fn get(p: *const {rust_ty}) -> {rust_ty} {{ unsafe {{ p.read_unaligned() }} }}
             fn main() {{ let bytes = [1u8, 2, 3, 4, 5, 6, 7, 8, 9];
             let f: unsafe fn(*const {rust_ty}) -> {rust_ty} = get;
             println!(\"{{}}\", unsafe {{ f(bytes.as_ptr().add(1).cast::<{rust_ty}>()) }}); }}"
        );
        let (hir, typed, mir) = front_end(&text);
        let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let mut loads = 0;
        for function in &module.functions {
            if !function.name.contains("read_unaligned") {
                continue;
            }
            for (_, inst) in function.layout_insts() {
                let inst = function.inst_data(inst);
                if let keel_ir::InstData::Load { ty, flags, .. } = inst {
                    assert_eq!(*ty, keel_ty);
                    assert_eq!(flags.align, 1);
                    assert!(!flags.notrap);
                    assert!(!flags.volatile);
                    loads += 1;
                }
            }
        }
        assert_eq!(loads, 1);
        keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
    }
}

#[test]
fn unsupported_substitutions_are_rejected_in_generic_wrappers() {
    for ty in [
        "i32",
        "bool",
        "char",
        "[u8; 4]",
        "(u16, u16)",
        "S",
        "D",
        "*const u8",
    ] {
        let text = format!(
            "struct S {{ x: u32 }} struct D {{ x: u32 }}
             impl Drop for D {{ fn drop(&mut self) {{}} }}
             unsafe fn get<T>(p: *const T) -> T {{ unsafe {{ core::ptr::read_unaligned(p) }} }}
             fn main() {{ let p = 0usize as *const {ty}; unsafe {{ let _ = get(p); }} }}"
        );
        let (hir, typed, mir) = front_end(&text);
        let errors = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_err();
        assert!(
            errors.iter().any(
                |d| d.message == "read_unaligned supports only u8, u16, u32, u64 and usize yet"
            ),
            "{ty}: {errors:?}"
        );
    }
}

#[test]
fn calls_require_unsafe_and_const_calls_are_explicitly_rejected() {
    for call in [
        "p.read_unaligned()",
        "(p as *mut u32).read_unaligned()",
        "core::ptr::read_unaligned(p)",
    ] {
        for (qualifier, expression, expected) in [
            (
                "",
                call.to_owned(),
                "unsafe and requires unsafe function or block",
            ),
            (
                "const ",
                format!("unsafe {{ {call} }}"),
                "cannot call non-const",
            ),
        ] {
            let text = format!(
                "{qualifier}fn get(p: *const u32) -> u32 {{ {expression} }} fn main() {{}}"
            );
            let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
            assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
            let typed = flux_mir::check_file(&hir);
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains(expected)),
                "{text}: {:?}",
                typed.diagnostics
            );
        }
    }
}
