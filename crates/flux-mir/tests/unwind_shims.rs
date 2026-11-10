//! Compiler-generated adapters retain propagation and partial-clone ownership.
use flux_mir::{Body, Operand, PanicStrategy, Rvalue, StatementKind, TerminatorKind, UnwindAction};
use flux_ty::{Instance, TyKind};
use std::collections::HashMap;

const SOURCE: &str = r#"
struct Captured(u32,u32);
impl Clone for Captured{fn clone(&self)->Self{if self.0==self.1{panic!("clone");}Captured(self.0+10,self.1)}}
impl Drop for Captured{fn drop(&mut self){if self.0>=10{println!("{}",self.0-10);}}}
fn clone_captures(fail:u32){let a=Captured(0,fail);let b=Captured(1,fail);let c=Captured(2,fail);let capture=move||{let _=(&a,&b,&c);};let _duplicate=capture.clone();}
fn target(){panic!("target");}
fn invoke<F:FnOnce()>(f:F){f();}
fn calls(){invoke(target);let pointer:fn()=target;invoke(pointer);}
"#;

#[test]
fn generated_shims_preserve_call_and_clone_cleanup() {
    let (_, _, hir) = flux_hir::lower_text("unwind_shims.rs", SOURCE);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let roots: Vec<_> = hir
        .items
        .functions
        .iter()
        .filter(|(id, f)| {
            hir.is_program(*id) && matches!(f.name.as_str(), "calls" | "clone_captures")
        })
        .map(|(function, _)| Instance {
            function,
            args: &[],
        })
        .collect();
    let collected = flux_mir::mono::collect_from_roots(&hir, &typed.sigs, &mir, &roots).unwrap();
    let clone_trait = hir.items.lang_traits[&flux_intern::Symbol::intern("clone")];
    let clone = hir.items.traits[clone_trait]
        .function(&hir.items, flux_intern::Symbol::intern("clone"))
        .unwrap();
    let drop_trait = hir.items.lang_traits[&flux_intern::Symbol::intern("drop")];
    let drop = hir.items.traits[drop_trait]
        .function(&hir.items, flux_intern::Symbol::intern("drop"))
        .unwrap();
    let root = std::env::temp_dir().join(format!("flux-unwind-clone-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("oracle.rs");
    let binary = root.join("oracle");
    std::fs::write(&path,format!("{SOURCE}\nfn main(){{std::panic::set_hook(Box::new(|_|{{}}));let fail=std::env::args().nth(1).unwrap().parse().unwrap();let _=std::panic::catch_unwind(||clone_captures(fail));}}")).unwrap();
    let result = std::process::Command::new("rustc")
        .args(["--edition=2024", "-Cpanic=unwind", "-o"])
        .arg(&binary)
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut seen_item = false;
    let mut seen_pointer = false;
    let mut seen_clone = false;
    for instance in collected.instances {
        let Some(self_ty) = instance.args.first() else {
            continue;
        };
        if flux_mir::fn_method_kind(&hir.items, instance.function).is_some()
            && matches!(self_ty.kind(), TyKind::FnDef(_) | TyKind::FnPtr(_))
        {
            let body = flux_mir::mono::body_of(&hir, &typed.sigs, &mir, instance);
            assert_eq!(
                body.blocks[Body::entry()].terminator.unwind,
                UnwindAction::Continue
            );
            seen_item |= matches!(self_ty.kind(), TyKind::FnDef(_));
            seen_pointer |= matches!(self_ty.kind(), TyKind::FnPtr(_));
        }
        if instance.function != clone || !matches!(self_ty.kind(), TyKind::Closure(..)) {
            continue;
        }
        let body = flux_mir::mono::body_of(&hir, &typed.sigs, &mir, instance);
        let errors = flux_mir::validate::validate(&hir.items, &typed.sigs, &[], &body);
        assert!(errors.is_empty(), "{errors:?}");
        let callbacks: Vec<_> = body
            .blocks
            .iter()
            .filter_map(|(block, data)| match &data.terminator.kind {
                TerminatorKind::Call { func, dest, .. } if *func == clone => {
                    Some((block, dest.local))
                }
                _ => None,
            })
            .collect();
        assert_eq!(callbacks.len(), 3);
        let indices: HashMap<_, _> = callbacks
            .iter()
            .enumerate()
            .map(|(index, (_, local))| (*local, index))
            .collect();
        let borrows: HashMap<_, _> = body
            .blocks
            .values()
            .flat_map(|data| data.statements.iter())
            .filter_map(|s| match &s.kind {
                StatementKind::Assign(dest, Rvalue::Ref { place, .. }) => {
                    Some((dest.local, place.local))
                }
                _ => None,
            })
            .collect();
        for (failure, (call, _)) in callbacks.iter().enumerate() {
            let mut actual = String::new();
            let mut next = match body.blocks[*call].terminator.unwind {
                UnwindAction::Continue => None,
                UnwindAction::Cleanup(target) => Some(target),
                other => panic!("clone exit {other:?}"),
            };
            while let Some(block) = next {
                let term = &body.blocks[block].terminator.kind;
                match term {
                    TerminatorKind::Goto(target) => next = Some(*target),
                    TerminatorKind::Call {
                        func, args, target, ..
                    } if *func == drop => {
                        let [Operand::Move(receiver)] = args.as_slice() else {
                            panic!("drop receiver")
                        };
                        let capture = borrows[&receiver.local];
                        actual.push_str(&format!("{}\n", indices[&capture]));
                        next = *target;
                    }
                    TerminatorKind::UnwindResume => next = None,
                    other => panic!("unexpected clone cleanup {other:?}"),
                }
            }
            let expected = std::process::Command::new(&binary)
                .arg(failure.to_string())
                .output()
                .unwrap();
            assert!(expected.status.success());
            assert_eq!(
                actual,
                String::from_utf8(expected.stdout).unwrap(),
                "failure at capture {failure}"
            );
        }
        seen_clone = true;
    }
    assert!(seen_item && seen_pointer && seen_clone);
    std::fs::remove_dir_all(root).unwrap();
}
