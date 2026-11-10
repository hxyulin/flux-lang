//! Foreign ABI boundaries distinguish cleanup from exception propagation.
use flux_hir::Abi;
use flux_mir::{PanicStrategy, TerminatorKind, UnwindAction};

const SOURCE: &str = r#"
struct Owned;
impl Drop for Owned { fn drop(&mut self) { eprintln!("drop"); } }
fn may_panic(){panic!("probe");}
unsafe extern "C" { fn foreign_c(); }
unsafe extern "C-unwind" { fn foreign_unwind(); }
unsafe extern "system" { fn foreign_system(); }
unsafe extern "system-unwind" { fn foreign_system_unwind(); }
fn direct(){
    let _owned=Owned;
    unsafe { foreign_c(); foreign_unwind(); foreign_system(); foreign_system_unwind(); }
}
fn indirect(c:unsafe extern "C" fn(),u:unsafe extern "C-unwind" fn(),s:unsafe extern "system" fn(),su:unsafe extern "system-unwind" fn()){
    let _owned=Owned;
    unsafe {c();u();s();su();}
}
extern "C" fn boundary_c(){let _owned=Owned;may_panic();}
extern "system" fn boundary_system(){let _owned=Owned;may_panic();}
extern "C-unwind" fn boundary_unwind(){let _owned=Owned;may_panic();}
extern "system-unwind" fn boundary_system_unwind(){let _owned=Owned;may_panic();}
extern "C" fn creates_closure(){let _closure=||{let _owned=Owned;may_panic();};}
"#;

#[test]
fn calls_and_cleanup_exits_follow_their_own_abi() {
    let (_, _, hir) = flux_hir::lower_text("unwind_abi.rs", SOURCE);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let errors = flux_mir::validate::validate_file(&hir, &typed.sigs, &mir);
    assert!(errors.is_empty(), "{errors:?}");
    let mut checked_calls = 0;
    let mut checked_boundaries = 0;
    for (id, body) in mir.bodies.iter().filter(|(id, _)| hir.is_program(*id)) {
        let name = hir.items.functions[id].name.as_str();
        if name == "direct" || name == "indirect" {
            for data in body.blocks.values() {
                let abi = match &data.terminator.kind {
                    TerminatorKind::Call { func, .. } if hir.items.functions[*func].is_extern => {
                        hir.items.functions[*func].abi
                    }
                    TerminatorKind::CallIndirect { callee, .. } => {
                        body.operand_ty(callee, &typed.sigs).fn_sig().unwrap().abi
                    }
                    _ => continue,
                };
                assert_eq!(
                    matches!(data.terminator.unwind, UnwindAction::Cleanup(_)),
                    matches!(abi, Abi::CUnwind | Abi::SystemUnwind),
                    "{name} {abi:?}"
                );
                checked_calls += 1;
            }
        }
        if name.starts_with("boundary_") {
            let escapes = hir.items.functions[id].abi.can_unwind();
            assert!(body.blocks.values().any(|data| matches!(
                data.terminator.kind,
                TerminatorKind::UnwindResume | TerminatorKind::UnwindTerminate
            )));
            for data in body.blocks.values() {
                match data.terminator.kind {
                    TerminatorKind::UnwindResume => assert!(escapes, "{name}"),
                    TerminatorKind::UnwindTerminate => assert!(!escapes, "{name}"),
                    _ => {}
                }
            }
            checked_boundaries += 1;
        }
    }
    assert_eq!(checked_calls, 8);
    assert_eq!(checked_boundaries, 4);
    let closure = mir
        .closures
        .iter()
        .find(|(id, body)| {
            body.upvars.is_some()
                && match id.0.owner {
                    flux_ty::ClosureOwner::Function(f) => {
                        hir.items.functions[f].name.as_str() == "creates_closure"
                    }
                    _ => false,
                }
        })
        .expect("Rust closure inside a C function")
        .1;
    assert!(
        closure
            .blocks
            .values()
            .any(|data| matches!(data.terminator.kind, TerminatorKind::UnwindResume))
    );
    assert!(
        !closure
            .blocks
            .values()
            .any(|data| matches!(data.terminator.kind, TerminatorKind::UnwindTerminate))
    );
    assert!(
        flux_mir::build_file(&hir, &typed)
            .bodies
            .iter()
            .all(|(_, body)| !body.has_unwind())
    );
}

#[test]
fn rustc_cleans_locals_before_terminating_a_nonunwind_boundary() {
    let root = std::env::temp_dir().join(format!("flux-unwind-abi-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("oracle.rs");
    std::fs::write(&source, format!("{SOURCE}\n{}", r#"
fn main(){
    std::panic::set_hook(Box::new(|_|{}));
    match std::env::args().nth(1).unwrap().as_str(){
        "C" => boundary_c(),
        "system" => boundary_system(),
        "C-unwind" => {assert!(std::panic::catch_unwind(||boundary_unwind()).is_err());eprintln!("caught");},
        "system-unwind" => {assert!(std::panic::catch_unwind(||boundary_system_unwind()).is_err());eprintln!("caught");},
        _ => unreachable!(),
    }
}
"#)).unwrap();
    let binary = root.join("oracle");
    let output = std::process::Command::new("rustc")
        .args(["--edition=2024", "-C", "panic=unwind"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (abi, catches) in [
        ("C", false),
        ("system", false),
        ("C-unwind", true),
        ("system-unwind", true),
    ] {
        let output = std::process::Command::new(&binary)
            .arg(abi)
            .output()
            .unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.success(), catches, "{abi}: {stderr}");
        assert_eq!(
            stderr.lines().filter(|line| *line == "drop").count(),
            1,
            "{abi}: {stderr}"
        );
        assert_eq!(
            stderr.lines().any(|line| line == "caught"),
            catches,
            "{abi}: {stderr}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
