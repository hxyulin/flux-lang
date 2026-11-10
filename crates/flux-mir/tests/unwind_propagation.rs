//! Panic strategies stay consistent across source, library and closure templates.
use flux_hir::Hir;
use flux_mir::{Body, PanicStrategy, TerminatorKind, UnwindAction};
use flux_ty::{ClosureKind, TypedFile};

const SOURCE: &str = r#"
struct Owned;
impl Drop for Owned {fn drop(&mut self){}}
fn may_panic(){panic!("probe");}
fn functions(){let _value=Owned;may_panic();}
fn shared(){let value=Owned;let _closure=move||{let _borrow=&value;may_panic();};}
fn mutable(){let value=Owned;let mut count=0;let _closure=move||{let _borrow=&value;count+=1;may_panic();};}
fn consuming(){let value=Owned;let _closure=move||{let _taken=value;may_panic();};}
fn pure(value:u32)->u32{value.count_ones()}
fn pointer_drop(value:*mut Owned){unsafe{core::ptr::drop_in_place(value);}}
fn memory(src:*const Owned,dst:*mut Owned){unsafe{let value=core::ptr::read(src);core::ptr::write(dst,value);}}
const REIFIED:fn()=||may_panic();
"#;

fn source() -> (Hir, TypedFile) {
    let (_, _, hir) = flux_hir::lower_text("unwind_propagation.rs", SOURCE);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    (hir, typed)
}

#[test]
fn source_libraries_and_closures_use_isolated_panic_strategies() {
    let (hir, typed) = source();
    let abort = flux_mir::build_file(&hir, &typed);
    assert!(abort.diagnostics.is_empty(), "{:?}", abort.diagnostics);
    let unwind = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert_eq!(unwind.panic_strategy, PanicStrategy::Unwind);
    let mut encoded = flux_hir::flib::Encoder::new();
    unwind.encode_flib(&mut encoded).unwrap();
    assert_eq!(&encoded.into_bytes()[..4], &u32::MAX.to_le_bytes());
    assert!(unwind.diagnostics.is_empty(), "{:?}", unwind.diagnostics);
    let again = flux_mir::build_file(&hir, &typed);
    assert!(again.diagnostics.is_empty(), "{:?}", again.diagnostics);
    let errors = flux_mir::validate::validate_file(&hir, &typed.sigs, &unwind);
    assert!(errors.is_empty(), "{errors:?}");
    let mut different_library = false;
    for (id, old) in abort.bodies.iter() {
        assert!(!old.has_unwind());
        let current = &again.bodies[id];
        assert_eq!(
            old.blocks.values().collect::<Vec<_>>(),
            current.blocks.values().collect::<Vec<_>>()
        );
        if !hir.is_program(id) && unwind.bodies.get(id).is_some_and(Body::has_unwind) {
            different_library = true;
        }
    }
    assert!(
        different_library,
        "embedded library functions must not reuse abort MIR in unwind mode"
    );
    for ((id, kind), body) in &unwind.closures {
        let user = match id.owner {
            flux_ty::ClosureOwner::Function(f) => hir.is_program(f),
            flux_ty::ClosureOwner::Const(c) => c.into_raw() >= hir.items.core.consts,
        };
        if !user {
            continue;
        }
        assert!(body.has_unwind(), "panic-capable closure {id:?} {kind:?}");
        if *kind == ClosureKind::FnOnce && body.upvars.is_none() {
            let UnwindAction::Cleanup(cleanup) = body.blocks[Body::entry()].terminator.unwind
            else {
                panic!("owning shim cleanup")
            };
            assert!(
                matches!(body.blocks[cleanup].terminator.kind,TerminatorKind::Drop{ref place,..} if place.local.into_raw()==1)
            );
            assert_eq!(
                body.blocks[cleanup].terminator.unwind,
                UnwindAction::Terminate
            );
        }
        let instance =
            flux_mir::mono::instance_body(&hir, &typed.sigs, &unwind.discriminants, body, &[]);
        assert!(flux_mir::validate::validate(&hir.items, &typed.sigs, &[], &instance).is_empty());
    }
    for (_, body) in unwind.consts.iter() {
        assert!(
            !body.has_unwind(),
            "constant evaluation uses abort semantics"
        );
    }
    assert!(again.closures.values().all(|body| !body.has_unwind()));
}

#[test]
fn pure_intrinsics_drop_exceptional_edges_but_calls_keep_them() {
    let (hir, typed) = source();
    let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let mut checked_count = false;
    for (id, body) in mir.bodies.iter() {
        let data = &hir.items.functions[id];
        if data.name.as_str() == "count_ones" {
            let expanded =
                flux_mir::mono::instance_body(&hir, &typed.sigs, &mir.discriminants, body, &[]);
            assert!(!expanded.has_unwind(), "count_ones cannot panic");
            checked_count = true;
        }
        if hir.is_program(id) && data.name.as_str() == "functions" {
            let expanded =
                flux_mir::mono::instance_body(&hir, &typed.sigs, &mir.discriminants, body, &[]);
            assert!(expanded.has_unwind());
            assert!(
                !expanded
                    .blocks
                    .values()
                    .any(|block| matches!(block.terminator.kind, TerminatorKind::Drop { .. }))
            );
            assert!(
                flux_mir::validate::validate(&hir.items, &typed.sigs, &[], &expanded).is_empty()
            );
        }
    }
    assert!(checked_count, "primitive method template must be reached");
}

#[test]
fn const_drop_checks_ignore_unwind_only_destruction() {
    let root = std::env::temp_dir().join(format!("flux-unwind-const-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "panic_path",
            "const fn f<T>(value:T,flag:bool){if flag{core::mem::forget(value);}else{panic!(\"const panic\");}}",
            true,
        ),
        ("normal_drop", "const fn f<T>(_value:T){}", false),
        ("never_returns", "const fn f<T>(_value:T){loop{}}", true),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "--out-dir",
            ])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty());
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{name}: {:?}",
            typed.diagnostics
        );
        for strategy in [PanicStrategy::Abort, PanicStrategy::Unwind] {
            let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, strategy);
            assert_eq!(
                mir.diagnostics.is_empty(),
                accepted,
                "{name}, {strategy:?}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn library_memory_wrappers_instantiate_with_correct_unwind_behavior() {
    let (hir, typed) = source();
    let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let memory = hir
        .items
        .functions
        .iter()
        .find(|(id, f)| hir.is_program(*id) && f.name.as_str() == "memory")
        .unwrap()
        .0;
    let flux_ty::TyKind::RawPtr(_, owned) = typed.sigs[memory].params[0].kind() else {
        panic!("owned pointee")
    };
    let mut seen = Vec::new();
    for (id, body) in mir.bodies.iter() {
        let function = &hir.items.functions[id];
        let name = function.name.as_str();
        if function.intrinsic.is_some()
            || function.owner.is_some()
            || hir.items.modules[function.loc.module].name.as_str() != "ptr"
            || !matches!(name, "read" | "write" | "drop_in_place")
        {
            continue;
        }
        let instance =
            flux_mir::mono::instance_body(&hir, &typed.sigs, &mir.discriminants, body, &[owned]);
        let errors = flux_mir::validate::validate(&hir.items, &typed.sigs, &[], &instance);
        assert!(errors.is_empty(), "{name}: {errors:?}");
        assert_eq!(instance.has_unwind(), name == "drop_in_place", "{name}");
        assert!(
            !instance
                .blocks
                .values()
                .any(|block| matches!(block.terminator.kind, TerminatorKind::Drop { .. })),
            "{name}"
        );
        seen.push(name.to_owned());
    }
    for expected in ["read", "write", "drop_in_place"] {
        assert!(
            seen.iter().any(|name| name == expected),
            "missing {expected}"
        );
    }
}
