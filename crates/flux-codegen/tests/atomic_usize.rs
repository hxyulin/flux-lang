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
    let (hir, typed, mir) = front_end(include_str!("../../../tests/run/atomic_usize.rs"));
    let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Full,
    ] {
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let mut operations = Vec::new();
        for function in &module.functions {
            for (_, inst) in function.layout_insts() {
                match function.inst_data(inst) {
                    InstData::AtomicLoad { ty, ordering, .. } => {
                        assert_eq!(*ty, Type::I64);
                        operations.push(("load", *ordering));
                    }
                    InstData::AtomicStore {
                        value, ordering, ..
                    } => {
                        assert_eq!(function.value_type(*value), Type::I64);
                        operations.push(("store", *ordering));
                    }
                    InstData::AtomicRmw {
                        op, ordering, args, ..
                    } => {
                        assert_eq!(function.value_type(args[1]), Type::I64);
                        operations.push((
                            if *op == keel_ir::RmwOp::Add {
                                "add"
                            } else {
                                "sub"
                            },
                            *ordering,
                        ));
                    }
                    InstData::AtomicCas { args, ordering, .. } => {
                        assert_eq!(function.value_type(args[1]), Type::I64);
                        assert_eq!(function.value_type(args[2]), Type::I64);
                        operations.push(("cas", *ordering));
                    }
                    InstData::Fence { ordering } => operations.push(("fence", *ordering)),
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
            (
                "fence",
                &[
                    Ordering::Acquire,
                    Ordering::Release,
                    Ordering::AcqRel,
                    Ordering::SeqCst,
                ][..],
            ),
            (
                "add",
                &[
                    Ordering::Relaxed,
                    Ordering::Acquire,
                    Ordering::Release,
                    Ordering::AcqRel,
                    Ordering::SeqCst,
                ][..],
            ),
            (
                "sub",
                &[
                    Ordering::Relaxed,
                    Ordering::Acquire,
                    Ordering::Release,
                    Ordering::AcqRel,
                    Ordering::SeqCst,
                ][..],
            ),
            (
                "cas",
                &[
                    Ordering::Relaxed,
                    Ordering::Acquire,
                    Ordering::Release,
                    Ordering::AcqRel,
                    Ordering::SeqCst,
                ][..],
            ),
        ] {
            for order in orders {
                assert!(operations.contains(&(op, *order)), "missing {op} {order:?}");
            }
        }
        let asm = keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
        for instruction in [
            "ldr\tx",
            "str\tx",
            "neg\tx",
            "ldar\tx",
            "stlr\tx",
            "ldadd\tx",
            "ldadda\tx",
            "ldaddl\tx",
            "ldaddal\tx",
            "cas\tx",
            "casa\tx",
            "casl\tx",
            "casal\tx",
            "dmb\tishld",
            "dmb\tish",
        ] {
            if instruction.starts_with("dmb") {
                assert!(
                    asm.lines().any(|line| line.trim() == instruction),
                    "missing {instruction}"
                );
            } else {
                assert!(asm.contains(instruction), "missing {instruction}");
            }
        }
        let object = flux_codegen::compile_object(
            &mut module,
            flux_codegen::Target::Aarch64AppleDarwin,
            keel_opt::OptimizationTier::None,
        )
        .unwrap();
        if let Some(path) = std::env::var_os("FLUX_ATOMIC_ARTIFACTS") {
            let dir = std::path::PathBuf::from(path);
            std::fs::create_dir_all(&dir).unwrap();
            let name = format!("{tier:?}");
            std::fs::write(dir.join(format!("{name}.ir")), module.to_string()).unwrap();
            std::fs::write(dir.join(format!("{name}.s")), asm).unwrap();
            std::fs::write(dir.join(format!("{name}.o")), object).unwrap();
        }
    }
}

/// The host Rust harness owns and pins four initialized, aligned words until
/// every thread joins. Counters and the ready flag use Flux atomic operations;
/// the plain payload is accessed only under release/acquire synchronization.
/// The object entry is a reified Flux function, hence uses the C scalar ABI.
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn contended_native_rmw_and_cas_from_rust_threads() {
    let text = r#"
use core::sync::atomic::{AtomicUsize, Ordering};
fn worker(p: *mut usize) -> usize {
    unsafe {
        let fetch = &*(p as *const AtomicUsize);
        let cas = &*(p.add(1) as *const AtomicUsize);
        let mut i = 0;
        while i < 20000 {
            fetch.fetch_add(1, Ordering::Relaxed);
            let mut old = cas.load(Ordering::Relaxed);
            loop {
                match cas.compare_exchange_weak(old, old + 1, Ordering::AcqRel, Ordering::Acquire) {
                    Ok(_) => break,
                    Err(observed) => old = observed,
                }
            }
            i += 1;
        }
        20000
    }
}
fn publish(p: *mut usize, mode: usize) -> usize {
    unsafe {
        let ready = &*(p.add(2) as *const AtomicUsize);
        *p.add(3) = 42;
        if mode == 0 { ready.store(1, Ordering::Release); }
        else {
            core::sync::atomic::fence(Ordering::Release);
            ready.store(1, Ordering::Relaxed);
        }
        42
    }
}
fn consume(p: *mut usize, mode: usize) -> usize {
    unsafe {
        let ready = &*(p.add(2) as *const AtomicUsize);
        if mode == 0 { while ready.load(Ordering::Acquire) == 0 {} }
        else {
            while ready.load(Ordering::Relaxed) == 0 {}
            core::sync::atomic::fence(Ordering::Acquire);
        }
        *p.add(3)
    }
}
fn main() {
    let mut words = [0usize; 4];
    let f: fn(*mut usize) -> usize = worker;
    let _ = f(words.as_mut_ptr());
    let writer: fn(*mut usize, usize) -> usize = publish;
    let reader: fn(*mut usize, usize) -> usize = consume;
    let _ = writer(words.as_mut_ptr(), 0);
    let _ = reader(words.as_mut_ptr(), 0);
}
"#;
    let (hir, typed, mir) = front_end(text);
    let original = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let symbol = original
        .functions
        .iter()
        .find(|f| f.name.contains("worker"))
        .unwrap()
        .name
        .clone();
    let worker = original
        .functions
        .iter()
        .find(|f| f.name == symbol)
        .unwrap();
    assert_eq!(worker.sig.cc, keel_ir::CallConv::C);
    assert_eq!(
        worker.sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
        [Type::Ptr]
    );
    assert_eq!(
        worker.sig.returns.iter().map(|p| p.ty).collect::<Vec<_>>(),
        [Type::I64]
    );
    let publisher = original
        .functions
        .iter()
        .find(|f| f.name.contains("publish"))
        .unwrap();
    let reader = original
        .functions
        .iter()
        .find(|f| f.name.contains("consume"))
        .unwrap();
    for function in [publisher, reader] {
        assert_eq!(function.sig.cc, keel_ir::CallConv::C);
        assert_eq!(
            function.sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
            [Type::Ptr, Type::I64]
        );
        assert_eq!(
            function
                .sig
                .returns
                .iter()
                .map(|p| p.ty)
                .collect::<Vec<_>>(),
            [Type::I64]
        );
    }
    let publish_symbol = publisher.name.clone();
    let consume_symbol = reader.name.clone();
    let dir = std::env::temp_dir().join(format!("flux-atomic-concurrency-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let driver = format!(
        r#"
use std::cell::UnsafeCell;
use std::sync::Arc;
#[repr(C, align(8))]
struct Storage(UnsafeCell<[usize; 4]>);
// SAFETY: counters and the ready flag are accessed atomically while shared.
// The single-writer payload is published by release/acquire synchronization.
// Ordinary host reads occur only after all threads join.
unsafe impl Sync for Storage {{}}
unsafe extern "C" {{
    #[link_name = "{symbol}"] fn worker(p: *mut usize) -> usize;
    #[link_name = "{publish_symbol}"] fn publish(p: *mut usize, mode: usize) -> usize;
    #[link_name = "{consume_symbol}"] fn consume(p: *mut usize, mode: usize) -> usize;
}}
fn main() {{
    assert_eq!(std::mem::size_of::<usize>(), 8);
    assert_eq!(std::mem::align_of::<Storage>(), 8);
    for _ in 0..4 {{
        let words = Arc::new(Storage(UnsafeCell::new([0; 4])));
        let handles: Vec<_> = (0..8).map(|_| {{
            let words = words.clone();
            std::thread::spawn(move || {{
                // SAFETY: Arc pins live initialized storage until this call returns.
                assert_eq!(unsafe {{ worker(words.0.get().cast::<usize>()) }}, 20000);
            }})
        }}).collect();
        for handle in handles {{ handle.join().unwrap(); }}
        // SAFETY: every concurrent access ended before the joins returned.
        assert_eq!(unsafe {{ *words.0.get() }}, [160000, 160000, 0, 0]);
        for mode in [0, 1] {{
            let storage = Arc::new(Storage(UnsafeCell::new([0; 4])));
            let sender = storage.clone();
            let receiver = storage.clone();
            // SAFETY: both Arcs pin the storage. The payload word has one
            // writer; release/acquire (or the corresponding fence pattern)
            // orders its write before the reader's ordinary access.
            let reader = std::thread::spawn(move || unsafe {{ consume(receiver.0.get().cast::<usize>(), mode) }});
            let writer = std::thread::spawn(move || unsafe {{ publish(sender.0.get().cast::<usize>(), mode) }});
            assert_eq!(writer.join().unwrap(), 42);
            assert_eq!(reader.join().unwrap(), 42);
        }}
    }}
}}
"#
    );
    std::fs::write(dir.join("driver.rs"), driver).unwrap();
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut module = original.clone();
        // The driver supplies the executable entry. Keep Flux's unused wrapper
        // under another symbol; no invalid program or null call is executed.
        for function in &mut module.functions {
            if function.name == symbol
                || function.name == publish_symbol
                || function.name == consume_symbol
            {
                function.local = false;
            }
            if function.name == "main" {
                function.name = "flux_unused_main".into();
            }
        }
        let object = flux_codegen::compile_object(
            &mut module,
            flux_codegen::Target::Aarch64AppleDarwin,
            tier,
        )
        .unwrap();
        std::fs::write(dir.join("atomic.o"), object).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2024", "-C", "opt-level=2"])
            .arg(dir.join("driver.rs"))
            .arg("-C")
            .arg(format!("link-arg={}", dir.join("atomic.o").display()))
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
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_rust_api_uses_are_compile_only() {
    let dir = std::env::temp_dir().join(format!("flux-atomic-api-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (i, source) in [
        "fn main() { let _ = core::sync::atomic::AtomicUsize::new(1u32); }",
        "#[repr(packed)] struct P { x: core::sync::atomic::AtomicUsize } fn main() { let p = P { x: core::sync::atomic::AtomicUsize::new(0) }; let _ = p.x.load(core::sync::atomic::Ordering::Relaxed); }",
        "fn main() { unsafe { core::intrinsics::__atomic_usize_load_relaxed(0usize as *const usize); } }",
        "use core::sync::atomic::Ordering; fn f(x: Ordering) { match x { Ordering::Relaxed => (), Ordering::Acquire => (), Ordering::Release => (), Ordering::AcqRel => (), Ordering::SeqCst => () } } fn main() {}",
        "fn sync<T: Sync>() {} fn main() { sync::<core::cell::UnsafeCell<usize>>(); }",
        "struct W<T> { counter: core::sync::atomic::AtomicUsize, payload: T } fn send<T: Send>() {} fn main() { send::<W<*mut usize>>(); }",
        "struct W<T> { counter: core::sync::atomic::AtomicUsize, payload: T } fn sync<T: Sync>() {} fn main() { sync::<W<core::cell::Cell<usize>>>(); }",
        "const fn read(a: &core::sync::atomic::AtomicUsize) -> usize { a.load(core::sync::atomic::Ordering::Relaxed) } fn main() {}",
    ].iter().enumerate() {
        let path = dir.join(format!("bad{i}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc").args(["--edition=2024", "--emit=metadata", "--out-dir"]).arg(&dir).arg(&path).output().unwrap();
        assert!(!output.status.success(), "rustc accepted {source}");
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let typed = flux_mir::check_file(&hir);
        assert!(!hir.diagnostics.is_empty() || !typed.diagnostics.is_empty(), "Flux accepted {source}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn invalid_runtime_orders_panic_before_access_at_the_caller() {
    let dir = std::env::temp_dir().join(format!("flux-atomic-panics-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (call, args, message) in [
        ("a.load(order);", "Ordering::Release", "a release load"),
        (
            "a.load(order);",
            "Ordering::AcqRel",
            "a acquire-release load",
        ),
        ("a.store(2, order);", "Ordering::Acquire", "a acquire store"),
        (
            "a.store(2, order);",
            "Ordering::AcqRel",
            "a acquire-release store",
        ),
        ("fence(order);", "Ordering::Relaxed", "a relaxed fence"),
        (
            "a.compare_exchange(1, 2, Ordering::Relaxed, order);",
            "Ordering::Release",
            "a release failure ordering",
        ),
        (
            "a.compare_exchange(1, 2, Ordering::Relaxed, order);",
            "Ordering::AcqRel",
            "an acquire-release failure ordering",
        ),
        (
            "a.compare_exchange_weak(1, 2, Ordering::SeqCst, order);",
            "Ordering::Release",
            "a release failure ordering",
        ),
        (
            "a.compare_exchange_weak(1, 2, Ordering::SeqCst, order);",
            "Ordering::AcqRel",
            "an acquire-release failure ordering",
        ),
    ] {
        // Runtime invalid order is legal Rust, and must panic. The parameter
        // prevents rustc's invalid_atomic_ordering lint on literal arguments.
        let text = format!(
            "use core::sync::atomic::{{AtomicUsize, Ordering, fence}};\n#[track_caller]\nfn bad(a: &AtomicUsize, order: Ordering) {{ {call} }}\nfn main() {{ bad(&AtomicUsize::new(1), {args}); }}\n"
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
    std::fs::remove_dir_all(dir).unwrap();
}
