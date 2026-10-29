#[path = "support/atomic.rs"]
mod atomic_support;
use keel_ir::{InstData, Ordering, Type};
#[test]
fn byte_cas_keeps_native_byte_access_and_ordering() {
    let (_, _, hir) = flux_hir::lower_text(
        "byte-cas.rs",
        r#"
        use core::sync::atomic::{AtomicU8,Ordering::*};
        static VALUE:AtomicU8=AtomicU8::new(0);
        fn main(){
            let a=VALUE.compare_exchange(0,1,Relaxed,Relaxed);
            let b=VALUE.compare_exchange(1,2,Acquire,Acquire);
            let c=VALUE.compare_exchange(2,3,Release,Relaxed);
            let d=VALUE.compare_exchange(3,4,AcqRel,Acquire);
            let e=VALUE.compare_exchange_weak(4,5,SeqCst,SeqCst);
            assert!(a.is_ok()&&b.is_ok()&&c.is_ok()&&d.is_ok()&&e.is_ok());
        }
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
        let mut orders = Vec::new();
        for function in atomic_support::named_functions(&module, "AtomicU8") {
            for (_, inst) in function.layout_insts() {
                if let InstData::AtomicCas { ordering, args } = function.inst_data(inst) {
                    assert_eq!(function.value_type(args[1]), Type::I8);
                    assert_eq!(function.value_type(args[2]), Type::I8);
                    orders.push(*ordering);
                }
            }
        }
        for order in [
            Ordering::Relaxed,
            Ordering::Acquire,
            Ordering::Release,
            Ordering::AcqRel,
            Ordering::SeqCst,
        ] {
            assert!(orders.contains(&order), "missing {order:?}");
        }
        let assembly =
            keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
        let assembly = atomic_support::function_assembly(
            &module,
            &atomic_support::named_functions(&module, "AtomicU8"),
            &assembly,
        );
        for instruction in ["casb", "casab", "caslb", "casalb"] {
            assert!(
                assembly.contains(instruction),
                "missing byte CAS mnemonic {instruction}"
            );
        }
        assert!(!assembly.contains("casal\tx"));
    }
}
#[test]
fn byte_atomic_ir_and_instruction_width_survive_optimization() {
    let (_, _, hir) = flux_hir::lower_text(
        "atomic-u8.rs",
        r#"
        use core::sync::atomic::{AtomicU8,Ordering::*};
        static VALUE:AtomicU8=AtomicU8::new(0);
        fn main(){
            VALUE.store(1,Relaxed);VALUE.store(2,Release);VALUE.store(3,SeqCst);
            let a=VALUE.load(Relaxed);let b=VALUE.load(Acquire);let c=VALUE.load(SeqCst);
            assert_eq!(a+b+c,9);
        }
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
        let mut ops = Vec::new();
        for function in atomic_support::named_functions(&module, "AtomicU8") {
            for (_, inst) in function.layout_insts() {
                match function.inst_data(inst) {
                    InstData::AtomicLoad { ty, ordering, .. } => {
                        assert_eq!(*ty, Type::I8, "{}", function.name);
                        ops.push(("load", *ordering));
                    }
                    InstData::AtomicStore {
                        value, ordering, ..
                    } => {
                        assert_eq!(function.value_type(*value), Type::I8);
                        ops.push(("store", *ordering));
                    }
                    _ => {}
                }
            }
        }
        for op in [
            ("load", Ordering::Relaxed),
            ("load", Ordering::Acquire),
            ("load", Ordering::SeqCst),
            ("store", Ordering::Relaxed),
            ("store", Ordering::Release),
            ("store", Ordering::SeqCst),
        ] {
            assert!(ops.contains(&op), "missing {op:?}");
        }
        let assembly =
            keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
        let assembly = atomic_support::function_assembly(
            &module,
            &atomic_support::named_functions(&module, "AtomicU8"),
            &assembly,
        );
        for instruction in ["ldrb\tw", "ldarb\tw", "strb\tw", "stlrb\tw"] {
            assert!(
                assembly.contains(instruction),
                "missing {instruction}\n{assembly}"
            );
        }
        assert!(!assembly.contains("ldar\tx"));
        assert!(!assembly.contains("stlr\tx"));
    }
}

#[test]
fn halfword_atomic_instructions_are_not_widened() {
    let (_, _, hir) = flux_hir::lower_text(
        "atomic-u16.rs",
        r#"
        use core::sync::atomic::{AtomicU16,Ordering::*};
        static VALUE:AtomicU16=AtomicU16::new(0);
        fn main(){VALUE.store(3,Relaxed);VALUE.store(5,Release);VALUE.store(7,SeqCst);assert_eq!(VALUE.load(Relaxed)+VALUE.load(Acquire)+VALUE.load(SeqCst),21);}
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
        let mut loads = 0;
        let mut stores = 0;
        for function in atomic_support::named_functions(&module, "AtomicU16") {
            for (_, inst) in function.layout_insts() {
                match function.inst_data(inst) {
                    InstData::AtomicLoad { ty, .. } => {
                        assert_eq!(*ty, Type::I16, "{}", function.name);
                        loads += 1;
                    }
                    InstData::AtomicStore { value, .. } => {
                        assert_eq!(function.value_type(*value), Type::I16);
                        stores += 1;
                    }
                    _ => {}
                }
            }
        }
        assert!(
            loads > 0 && stores > 0,
            "no halfword atomic load/store coverage"
        );
        let assembly =
            keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
        let assembly = atomic_support::function_assembly(
            &module,
            &atomic_support::named_functions(&module, "AtomicU16"),
            &assembly,
        );
        for op in ["ldrh\tw", "ldarh\tw", "strh\tw", "stlrh\tw"] {
            assert!(assembly.contains(op), "missing {op}");
        }
    }
}
