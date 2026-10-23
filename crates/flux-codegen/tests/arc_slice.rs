#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use keel_ir::{CallConv, Type};

fn lower(text: &str) -> keel_ir::Module {
    let (_, _, hir) = flux_hir::lower_text("main.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    flux_codegen::lower_module(&hir, &typed, &mir).unwrap()
}

#[test]
fn native_slice_ownership_contention_and_last_owner_visibility() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/arc-slice/native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = include_str!("fixtures/arc_slice/native.rs.txt");
    std::fs::write(dir.join("native.rs"), source).unwrap();
    // Validate the FFI source with rustc too; no invalid Rust is executed.
    let output = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(dir.join("native.rs"))
        .arg("-o")
        .arg(dir.join("native.rmeta"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut original = lower(source);
    // Audit callbacks and redirected libc hooks as well as host-callable entries.
    for (name, params, returns) in [
        ("arc_test_dropped", vec![Type::I64], vec![]),
        ("malloc", vec![Type::I64], vec![Type::Ptr]),
        ("free", vec![Type::Ptr], vec![]),
    ] {
        let declaration = original.decls.iter().find(|d| d.name == name).unwrap();
        assert_eq!(declaration.sig.cc, CallConv::C);
        assert_eq!(
            declaration
                .sig
                .params
                .iter()
                .map(|p| p.ty)
                .collect::<Vec<_>>(),
            params
        );
        assert_eq!(
            declaration
                .sig
                .returns
                .iter()
                .map(|p| p.ty)
                .collect::<Vec<_>>(),
            returns
        );
    }
    let mut declarations = String::from("unsafe extern \"C\" {\n");
    for (name, params, result) in [
        ("arc_create", vec![], Type::Ptr),
        ("empty_weak_control", vec![], Type::I64),
        ("arc_clone", vec![Type::Ptr], Type::Ptr),
        ("arc_downgrade", vec![Type::Ptr], Type::Ptr),
        ("weak_clone", vec![Type::Ptr], Type::Ptr),
        ("weak_upgrade", vec![Type::Ptr], Type::Ptr),
        ("arc_release", vec![Type::Ptr], Type::I64),
        ("weak_release", vec![Type::Ptr], Type::I64),
        ("arc_write_release", vec![Type::Ptr, Type::I64], Type::I64),
    ] {
        let functions: Vec<_> = original
            .functions
            .iter()
            .filter(|f| f.name.contains(name))
            .collect();
        assert_eq!(functions.len(), 1, "ambiguous {name}");
        let function = functions[0];
        assert_eq!(function.sig.cc, CallConv::C);
        assert_eq!(
            function.sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
            params
        );
        assert_eq!(
            function
                .sig
                .returns
                .iter()
                .map(|p| p.ty)
                .collect::<Vec<_>>(),
            [result]
        );
        let args = params
            .iter()
            .enumerate()
            .map(|(i, t)| {
                format!(
                    "p{i}: {}",
                    if *t == Type::Ptr { "*mut u8" } else { "usize" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        declarations.push_str(&format!(
            "#[link_name = {:?}] fn {name}({args}) -> {};\n",
            function.name,
            if result == Type::Ptr {
                "*mut u8"
            } else {
                "usize"
            }
        ));
        let symbol = function.name.clone();
        original
            .functions
            .iter_mut()
            .find(|f| f.name == symbol)
            .unwrap()
            .local = false;
    }
    declarations.push_str("}\n");
    std::fs::write(
        dir.join("host.rs"),
        format!(
            "{declarations}{}",
            include_str!("fixtures/arc_slice/host.rs.txt")
        ),
    )
    .unwrap();
    // Only allocator symbol names are redirected in this test object. The
    // production Arc operations, layouts, counters and orderings are unchanged.
    for decl in &mut original.decls {
        if decl.name == "malloc" {
            decl.name = "arc_test_malloc".into();
        }
        if decl.name == "free" {
            decl.name = "arc_test_free".into();
        }
    }
    for function in &mut original.functions {
        if function.name == "main" {
            function.name = "flux_unused_main".into();
        }
        let replacements: Vec<_> = function
            .callees()
            .filter_map(|(id, callee)| match callee.name.as_str() {
                "malloc" => Some((id, "arc_test_malloc")),
                "free" => Some((id, "arc_test_free")),
                _ => None,
            })
            .collect();
        for (id, name) in replacements {
            function.callee_mut(id).name = name.into();
        }
    }
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut module = original.clone();
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let ir = module.to_string();
        std::fs::write(dir.join(format!("{tier:?}.ir")), &ir).unwrap();
        // Confirm the actual library emits native release decrements and
        // acquire upgrade CAS/fences rather than plain shared counter accesses.
        let mut release_sub = false;
        let mut acquire_cas = false;
        let mut relaxed_cas = false;
        let mut acquire_fence = false;
        for function in &module.functions {
            for (_, inst) in function.layout_insts() {
                match function.inst_data(inst) {
                    keel_ir::InstData::AtomicRmw {
                        op: keel_ir::RmwOp::Sub,
                        ordering: keel_ir::Ordering::Release,
                        ..
                    } => release_sub = true,
                    keel_ir::InstData::AtomicCas {
                        ordering: keel_ir::Ordering::Acquire,
                        ..
                    } => acquire_cas = true,
                    keel_ir::InstData::AtomicCas {
                        ordering: keel_ir::Ordering::Relaxed,
                        ..
                    } => relaxed_cas = true,
                    keel_ir::InstData::Fence {
                        ordering: keel_ir::Ordering::Acquire,
                    } => acquire_fence = true,
                    _ => {}
                }
            }
        }
        assert!(release_sub && acquire_cas && relaxed_cas && acquire_fence);
        let object = flux_codegen::compile_object(
            &mut module,
            flux_codegen::Target::Aarch64AppleDarwin,
            keel_opt::OptimizationTier::None,
        )
        .unwrap();
        std::fs::write(dir.join("arc.o"), object).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2024", "-C", "opt-level=2"])
            .arg(dir.join("host.rs"))
            .arg("-C")
            .arg(format!("link-arg={}", dir.join("arc.o").display()))
            .arg("-o")
            .arg(dir.join("host"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = std::process::Command::new(dir.join("host"))
            .output()
            .unwrap();
        std::fs::write(dir.join(format!("{tier:?}.stderr")), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
