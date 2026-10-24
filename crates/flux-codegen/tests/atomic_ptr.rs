use keel_ir::{InstData, Ordering, Type};

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
fn native_atomic_ir_and_instructions_survive_optimization() {
    let (hir, typed, mir) = front_end(include_str!("../../../tests/run/atomic_ptr.rs"));
    let original = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut module = original.clone();
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        keel_ir::verify_module(&module).unwrap();
        // The load/store methods themselves carry pointers throughout. A null
        // constructor and unrelated formatting may materialize pointer constants.
        for f in &module.functions {
            if f.name.contains("AtomicPtr4load") || f.name.contains("AtomicPtr5store") {
                for (_, inst) in f.layout_insts() {
                    match f.inst_data(inst) {
                        InstData::Convert {
                            op: keel_ir::ConvertOp::PtrToInt,
                            ..
                        } => panic!("pointer-to-integer atomic bridge: {f}"),
                        InstData::Convert {
                            op: keel_ir::ConvertOp::IntToPtr,
                            arg,
                            ..
                        } => {
                            // Inlining panic formatting may materialize a null
                            // pointer, but never convert a dynamic integer payload.
                            let keel_ir::ValueDef::Result(def, _) = f.value_def(f.resolve(*arg))
                            else {
                                panic!("integer-to-pointer atomic bridge: {f}");
                            };
                            assert!(
                                matches!(f.inst_data(def), InstData::Iconst { imm: 0, .. }),
                                "integer-to-pointer atomic bridge: {f}"
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut operations = Vec::new();
        for function in &module.functions {
            for (_, inst) in function.layout_insts() {
                match function.inst_data(inst) {
                    InstData::AtomicLoad { ty, ordering, .. } => {
                        assert_eq!(*ty, Type::Ptr);
                        operations.push(("load", *ordering));
                    }
                    InstData::AtomicStore {
                        value, ordering, ..
                    } => {
                        assert_eq!(function.value_type(*value), Type::Ptr);
                        operations.push(("store", *ordering));
                    }
                    _ => {}
                }
            }
        }
        for (op, orders) in [
            (
                "load",
                &[Ordering::Relaxed, Ordering::Acquire, Ordering::SeqCst][..],
            ),
            (
                "store",
                &[Ordering::Relaxed, Ordering::Release, Ordering::SeqCst][..],
            ),
        ] {
            for order in orders {
                assert!(operations.contains(&(op, *order)), "missing {op} {order:?}");
            }
        }
        let asm = keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
        for instruction in ["ldr\tx", "str\tx", "ldar\tx", "stlr\tx"] {
            assert!(asm.contains(instruction), "missing {instruction}");
        }
        for instruction in ["ldar\tw", "stlr\tw"] {
            assert!(
                !asm.contains(instruction),
                "narrowed pointer atomic {instruction}"
            );
        }
        let object = flux_codegen::compile_object(
            &mut module,
            flux_codegen::Target::Aarch64AppleDarwin,
            keel_opt::OptimizationTier::None,
        )
        .unwrap();
        if let Some(path) = std::env::var_os("FLUX_ATOMIC_PTR_ARTIFACTS") {
            let dir = std::path::PathBuf::from(path);
            std::fs::create_dir_all(&dir).unwrap();
            let name = format!("{tier:?}");
            std::fs::write(dir.join(format!("{name}.ir")), module.to_string()).unwrap();
            std::fs::write(dir.join(format!("{name}.s")), asm).unwrap();
            std::fs::write(dir.join(format!("{name}.o")), object).unwrap();
        }
    }
}

#[test]
fn invalid_rust_api_uses_are_compile_only() {
    let dir = std::env::temp_dir().join(format!("flux-atomic-ptr-api-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (i, source) in [
        "fn main() { let _ = core::sync::atomic::AtomicPtr::<u8>::new(1usize); }",
        "fn f(_: core::sync::atomic::AtomicPtr<str>) {} fn main() {}",
        "fn main() { let _ = core::sync::atomic::AtomicPtr::<[u8]>::new(core::ptr::null_mut()); }",
        "fn main() { unsafe { core::intrinsics::__atomic_ptr_load_relaxed(core::ptr::null::<*mut u8>()); } }",
        "const fn f(a: &core::sync::atomic::AtomicPtr<u8>) -> *mut u8 { a.load(core::sync::atomic::Ordering::Relaxed) } fn main() {}",
        "fn main() { let a = core::sync::atomic::AtomicPtr::<u8>::new(core::ptr::null_mut()); let _ = a.get_mut(); }",
        "fn shorten<'a>(p: core::sync::atomic::AtomicPtr<&'static u8>, _: &'a u8) -> core::sync::atomic::AtomicPtr<&'a u8> { p } fn main() {}",
        "fn f<'a>(p: *mut &'a u8) -> &'static u8 { unsafe { *core::sync::atomic::AtomicPtr::new(p).load(core::sync::atomic::Ordering::Relaxed) } } fn main() {}",
        "fn f(a: &core::sync::atomic::AtomicPtr<u8>) -> &mut *mut u8 { a.get_mut() } fn main() {}",
    ].iter().enumerate() {
        let path = dir.join(format!("bad{i}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc").args(["--edition=2024", "--emit=metadata", "--out-dir"]).arg(&dir).arg(&path).output().unwrap();
        assert!(!output.status.success(), "rustc accepted {source}");
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let typed = flux_mir::check_file(&hir);
        if hir.diagnostics.is_empty() && typed.diagnostics.is_empty() {
            let mir = flux_mir::build_file(&hir, &typed);
            assert!(!mir.diagnostics.is_empty(), "Flux accepted {source}");
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn invalid_runtime_orders_panic_before_access_at_the_caller() {
    let dir = std::env::temp_dir().join(format!("flux-atomic-ptr-panics-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (call, args, message) in [
        ("a.load(order);", "Ordering::Release", "a release load"),
        (
            "a.load(order);",
            "Ordering::AcqRel",
            "a acquire-release load",
        ),
        (
            "a.store(core::ptr::null_mut(), order);",
            "Ordering::Acquire",
            "a acquire store",
        ),
        (
            "a.store(core::ptr::null_mut(), order);",
            "Ordering::AcqRel",
            "a acquire-release store",
        ),
    ] {
        // Runtime invalid order is legal Rust, and must panic. The parameter
        // prevents rustc's invalid_atomic_ordering lint on literal arguments.
        let text = format!(
            "use core::sync::atomic::{{AtomicPtr, Ordering}};\n#[track_caller]\nfn bad(a: &AtomicPtr<u8>, order: Ordering) {{ {call} }}\nfn main() {{ bad(&AtomicPtr::new(core::ptr::null_mut()), {args}); }}\n"
        );
        let (hir, typed, mir) = front_end(&text);
        let path = dir.join("main.rs");
        std::fs::write(&path, &text).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&path)
            .arg("-o")
            .arg(dir.join("rustc"))
            .output()
            .unwrap();
        assert!(
            rustc.status.success(),
            "{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let rust = std::process::Command::new(dir.join("rustc"))
            .output()
            .unwrap();
        assert!(!rust.status.success());
        // Match exact std wording (including its choice of a/an).
        let expected = message.replace("a acquire", "an acquire");
        assert!(String::from_utf8_lossy(&rust.stderr).contains(&expected));
        for tier in [
            keel_opt::OptimizationTier::None,
            keel_opt::OptimizationTier::Full,
        ] {
            let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
            let object = flux_codegen::compile_object(
                &mut module,
                flux_codegen::Target::Aarch64AppleDarwin,
                tier,
            )
            .unwrap();
            flux_codegen::link_executable(
                &object,
                flux_codegen::Target::Aarch64AppleDarwin,
                &dir.join("flux"),
            )
            .unwrap();
            let output = std::process::Command::new(dir.join("flux"))
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success());
            assert!(stderr.contains(&expected), "{stderr}");
            assert!(stderr.contains("main.rs:4:"), "{stderr}");
        }
    }
}

#[test]
fn bridge_names_without_intrinsic_identity_remain_ordinary_calls() {
    let text = "fn __atomic_ptr_load_relaxed<T>(_: *const T) -> u32 { 19 } fn main() { println!(\"{}\", __atomic_ptr_load_relaxed(&7u32)); }";
    let (hir, typed, mir) = front_end(text);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for f in &module.functions {
        for (_, inst) in f.layout_insts() {
            assert!(!matches!(
                f.inst_data(inst),
                InstData::AtomicLoad { .. } | InstData::AtomicStore { .. }
            ));
        }
    }
}

/// The Rust driver owns aligned pointer storage inside an Arc. Only Flux's
/// atomic functions access its slot until all threads join. The pointees stay
/// live, and the publication payload has one writer ordered before its reader.
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn native_publication_and_contended_pointer_identity() {
    let text = r#"
use core::sync::atomic::{AtomicPtr, Ordering};
fn publish(slot: *mut *mut u64, payload: *mut u64) -> usize {
    unsafe {
        let a = &*(slot as *const AtomicPtr<u64>);
        *payload = 0x123456789abcdef0;
        a.store(payload, Ordering::Release);
        1
    }
}
fn consume(slot: *mut *mut u64, payload: *mut u64) -> usize {
    unsafe {
        let a = &*(slot as *const AtomicPtr<u64>);
        let mut p = a.load(Ordering::Acquire);
        while p.is_null() { p = a.load(Ordering::Acquire); }
        if p == payload && *p == 0x123456789abcdef0 { 1 } else { 0 }
    }
}
fn contend(slot: *mut *mut u64, first: *mut u64, second: *mut u64) -> usize {
    unsafe {
        let a = &*(slot as *const AtomicPtr<u64>);
        let mut i = 0;
        while i < 20000 {
            a.store(first, Ordering::Release);
            let p = a.load(Ordering::Acquire);
            if p != first && p != second { return 0; }
            a.store(second, Ordering::SeqCst);
            i += 1;
        }
        1
    }
}
fn main() {
    let mut slot = core::ptr::null_mut();
    let mut first = 0u64;
    let mut second = 1u64;
    let p: fn(*mut *mut u64, *mut u64) -> usize = publish;
    let c: fn(*mut *mut u64, *mut u64) -> usize = consume;
    let w: fn(*mut *mut u64, *mut u64, *mut u64) -> usize = contend;
    println!("{} {} {}", p(&mut slot, &mut first), c(&mut slot, &mut first), w(&mut slot, &mut first, &mut second));
}
"#;
    let (hir, typed, mir) = front_end(text);
    let original = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let mut symbols = Vec::new();
    for (name, arity) in [("publish", 2), ("consume", 2), ("contend", 3)] {
        let f = original
            .functions
            .iter()
            .find(|f| f.name.contains(name))
            .unwrap();
        assert_eq!(f.sig.cc, keel_ir::CallConv::C);
        assert_eq!(
            f.sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
            vec![Type::Ptr; arity]
        );
        assert_eq!(
            f.sig.returns.iter().map(|p| p.ty).collect::<Vec<_>>(),
            [Type::I64]
        );
        symbols.push(f.name.clone());
    }
    let dir = std::env::temp_dir().join(format!("flux-atomic-ptr-threads-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let driver = format!(
        r#"
use std::cell::UnsafeCell;
use std::sync::Arc;
#[repr(C)]
struct Storage {{ before: u64, slot: UnsafeCell<*mut u64>, after: u64, payload: UnsafeCell<u64>, second: UnsafeCell<u64> }}
// SAFETY: the slot is only accessed through Flux atomics while shared. The
// payload has one writer and release/acquire publication; contend never reads
// either pointee. Host reads happen only after joins; Arcs keep everything live.
unsafe impl Sync for Storage {{}}
// SAFETY: moving storage transfers no access to its pointees.
unsafe impl Send for Storage {{}}
unsafe extern "C" {{
    #[link_name="{}"] fn publish(s: *mut *mut u64, p: *mut u64) -> usize;
    #[link_name="{}"] fn consume(s: *mut *mut u64, p: *mut u64) -> usize;
    #[link_name="{}"] fn contend(s: *mut *mut u64, a: *mut u64, b: *mut u64) -> usize;
}}
fn storage() -> Arc<Storage> {{ Arc::new(Storage {{ before: 0x1122334455667788, slot: UnsafeCell::new(std::ptr::null_mut()), after: 0x8877665544332211, payload: UnsafeCell::new(0), second: UnsafeCell::new(1) }}) }}
fn main() {{
    assert_eq!(std::mem::size_of::<*mut u64>(), 8);
    assert_eq!(std::mem::align_of::<Storage>(), 8);
    assert_eq!(std::mem::offset_of!(Storage, slot), 8);
    assert_eq!(std::mem::offset_of!(Storage, after), 16);
    for _ in 0..16 {{
        let s = storage();
        let a = s.clone(); let b = s.clone();
        // SAFETY: pinned initialized storage and synchronized single-writer payload.
        let reader = std::thread::spawn(move || unsafe {{ consume(a.slot.get(), a.payload.get()) }});
        // SAFETY: same storage, with only this thread writing the payload.
        let writer = std::thread::spawn(move || unsafe {{ publish(b.slot.get(), b.payload.get()) }});
        assert_eq!(writer.join().unwrap(), 1);
        assert_eq!(reader.join().unwrap(), 1);
        let handles: Vec<_> = (0..8).map(|_| {{ let s = s.clone();
            // SAFETY: all accesses to the slot are atomic; no pointee is read.
            std::thread::spawn(move || unsafe {{ contend(s.slot.get(), s.payload.get(), s.second.get()) }})
        }}).collect();
        for h in handles {{ assert_eq!(h.join().unwrap(), 1); }}
        assert_eq!(s.before, 0x1122334455667788);
        assert_eq!(s.after, 0x8877665544332211);
        // SAFETY: every accessing thread joined; pointer retains exact identity.
        let p = unsafe {{ *s.slot.get() }};
        assert!(p == s.payload.get() || p == s.second.get());
        // SAFETY: all threads joined before the host reads the payload.
        assert_eq!(unsafe {{ *s.payload.get() }}, 0x123456789abcdef0);
    }}
}}
"#,
        symbols[0], symbols[1], symbols[2]
    );
    std::fs::write(dir.join("driver.rs"), driver).unwrap();
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut module = original.clone();
        for f in &mut module.functions {
            if symbols.contains(&f.name) {
                f.local = false;
            }
            if f.name == "main" {
                f.name = "flux_unused_main".into();
            }
        }
        let object = flux_codegen::compile_object(
            &mut module,
            flux_codegen::Target::Aarch64AppleDarwin,
            tier,
        )
        .unwrap();
        std::fs::write(dir.join(format!("{tier:?}.o")), &object).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2024", "-C", "opt-level=2"])
            .arg(dir.join("driver.rs"))
            .arg("-C")
            .arg(format!(
                "link-arg={}",
                dir.join(format!("{tier:?}.o")).display()
            ))
            .arg("-o")
            .arg(dir.join("driver"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            std::process::Command::new(dir.join("driver"))
                .status()
                .unwrap()
                .success()
        );
    }
}
