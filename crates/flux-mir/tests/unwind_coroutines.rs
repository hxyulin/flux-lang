//! Coroutine lifting preserves exception exits, poisoning and deallocation order.
use flux_hir::Intrinsic;
use flux_mir::{
    BasicBlock, Body, Constant, Operand, PanicStrategy, ProjectionElem, Rvalue, StatementKind,
    TerminatorKind,
};
use flux_ty::Instance;
use std::collections::HashSet;

const SOURCE: &str = r#"
use core::future::Future;
struct Owned;
impl Drop for Owned{fn drop(&mut self){}}
async fn empty(){}
async fn body_panic(){let _value=Owned;panic!("body");}
async fn waiting(){let _value=Owned;core::future::pending::<()>().await;panic!("after await");}
fn captures()->impl Future<Output=()>{let owned=Owned;async move{let _value=owned;core::future::pending::<()>().await;}}
async fn partial_move(){let pair=(Owned,Owned);drop(pair.0);core::future::pending::<()>().await;}
async fn owned_output()->Owned{let _value=Owned;Owned}
async fn string_output()->String{let _value=Owned;String::from("value")}
async fn looping(){for _index in 0..2 {let _value=Owned;core::future::pending::<()>().await;}}
"#;

fn state_write(body: &Body, kind: &StatementKind) -> Option<u128> {
    let StatementKind::Assign(place, Rvalue::Use(Operand::Const(Constant::Int { bits, .. }))) =
        kind
    else {
        return None;
    };
    (place.local.index() == body.param_count + 1
        && place.projection.as_ref() == [ProjectionElem::Deref, ProjectionElem::Field(0)])
    .then_some(*bits)
}

fn dispatch(body: &Body, value: u128) -> BasicBlock {
    let TerminatorKind::SwitchInt {
        targets, otherwise, ..
    } = &body.blocks[Body::entry()].terminator.kind
    else {
        panic!("state dispatch")
    };
    targets
        .iter()
        .find(|(state, _)| *state == value)
        .map(|(_, target)| *target)
        .unwrap_or(*otherwise)
}

fn assert_free_on_every_exit(body: &Body, free: flux_hir::FunctionId) {
    let mut pending = vec![(Body::entry(), 0usize)];
    let mut seen = HashSet::new();
    while let Some((block, mut frees)) = pending.pop() {
        if !seen.insert((block, frees)) {
            continue;
        }
        let data = &body.blocks[block];
        if frees > 0 {
            assert!(
                !matches!(
                    data.terminator.kind,
                    TerminatorKind::Call { .. }
                        | TerminatorKind::CallIndirect { .. }
                        | TerminatorKind::Drop { .. }
                ),
                "call after freeing frame in bb{}",
                block.index()
            );
            for statement in &data.statements {
                assert!(
                    matches!(&statement.kind,StatementKind::Assign(place,Rvalue::Use(Operand::Const(Constant::Unit))) if place.local==Body::RETURN),
                    "frame access after deallocation: {statement:?}"
                );
            }
        }
        if matches!(data.terminator.kind,TerminatorKind::Call{func,..}if func==free) {
            frees += 1;
        }
        assert!(frees <= 1, "frame freed twice");
        if matches!(
            data.terminator.kind,
            TerminatorKind::Return | TerminatorKind::UnwindResume
        ) {
            assert_eq!(frees, 1, "exit without deallocation in bb{}", block.index());
        }
        pending.extend(data.terminator.successors().map(|target| (target, frees)));
    }
}

#[test]
fn coroutine_poll_and_drop_preserve_unwind_state() {
    let native_messages = native_inactive_messages();
    let (_, _, hir) = flux_hir::lower_text("unwind_coroutines.rs", SOURCE);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let find = |intrinsic| {
        hir.items
            .functions
            .iter()
            .find(|(_, f)| f.intrinsic == Some(intrinsic))
            .unwrap()
            .0
    };
    let poll = find(Intrinsic::CoroutinePoll);
    let drop = find(Intrinsic::CoroutineDrop);
    let free = hir.items.lang_fns[&flux_intern::Symbol::intern("coroutine_free")];
    let mut factories = 0;
    for (owner, body) in mir.bodies.iter().filter(|(id, _)| hir.is_program(*id)) {
        for block in body.blocks.values() {
            let TerminatorKind::Call {
                func, generic_args, ..
            } = block.terminator.kind
            else {
                continue;
            };
            if hir.items.functions[func].intrinsic != Some(Intrinsic::CoroutineNew) {
                continue;
            }
            factories += 1;
            let collected = flux_mir::mono::collect_from_roots(
                &hir,
                &typed.sigs,
                &mir,
                &[
                    Instance {
                        function: poll,
                        args: generic_args,
                    },
                    Instance {
                        function: drop,
                        args: generic_args,
                    },
                ],
            )
            .unwrap();
            assert!(
                !collected
                    .instances
                    .iter()
                    .any(|instance| hir.items.functions[instance.function].intrinsic
                        == Some(Intrinsic::CoroutineAwait)),
                "await must be lowered before collection"
            );
            let polled = flux_mir::mono::body_of(
                &hir,
                &typed.sigs,
                &mir,
                Instance {
                    function: poll,
                    args: generic_args,
                },
            );
            let errors = flux_mir::validate::validate(&hir.items, &typed.sigs, &[], &polled);
            assert!(
                errors.is_empty(),
                "{} poll: {errors:?}",
                hir.items.functions[owner].name
            );
            for data in polled.blocks.values() {
                if matches!(data.terminator.kind, TerminatorKind::UnwindResume) {
                    assert!(
                        data.statements.iter().any(
                            |s| state_write(&polled, &s.kind) == Some(u128::from(u64::MAX - 1))
                        ),
                        "resume must poison frame"
                    );
                }
            }
            for (state, message) in [u64::MAX, u64::MAX - 1].into_iter().zip(&native_messages) {
                let target = dispatch(&polled, u128::from(state));
                let data = &polled.blocks[target];
                let TerminatorKind::Call { func, args, .. } = &data.terminator.kind else {
                    panic!("inactive poll panics")
                };
                assert_eq!(Some(*func), hir.items.panic_fn);
                assert_eq!(
                    data.terminator.unwind,
                    flux_mir::UnwindAction::Continue,
                    "inactive poll must not reenter cleanup"
                );
                assert!(
                    data.statements
                        .iter()
                        .all(|s| state_write(&polled, &s.kind).is_none()),
                    "repoll must preserve original state"
                );
                let [Operand::Const(Constant::Promoted { index, .. })] = args.as_slice() else {
                    panic!("panic message")
                };
                let flux_mir::eval::Value::Aggregate(bytes) = &polled.promoted[*index as usize]
                else {
                    panic!("message bytes")
                };
                let actual: Vec<_> = bytes
                    .iter()
                    .map(|value| {
                        let flux_mir::eval::Value::Int { bits, .. } = value else {
                            panic!("byte")
                        };
                        *bits as u8
                    })
                    .collect();
                assert_eq!(&String::from_utf8(actual).unwrap(), message);
            }
            let dropped = flux_mir::mono::body_of(
                &hir,
                &typed.sigs,
                &mir,
                Instance {
                    function: drop,
                    args: generic_args,
                },
            );
            let errors = flux_mir::validate::validate(&hir.items, &typed.sigs, &[], &dropped);
            assert!(
                errors.is_empty(),
                "{} drop: {errors:?}",
                hir.items.functions[owner].name
            );
            assert_free_on_every_exit(&dropped, free);
            for state in [u64::MAX, u64::MAX - 1] {
                let target = dispatch(&dropped, u128::from(state));
                assert!(
                    matches!(dropped.blocks[target].terminator.kind,TerminatorKind::Call{func,..}if func==free),
                    "inactive frames only deallocate"
                );
            }
        }
    }
    assert_eq!(factories, 8);
}

fn native_inactive_messages() -> Vec<String> {
    let root =
        std::env::temp_dir().join(format!("flux-unwind-async-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("oracle.rs");
    let binary = root.join("oracle");
    let driver = r#"
fn main(){
    std::panic::set_hook(Box::new(|_|{}));
    let mut cx=std::task::Context::from_waker(std::task::Waker::noop());
    let mut done=Box::pin(empty());assert!(done.as_mut().poll(&mut cx).is_ready());
    let payload=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||done.as_mut().poll(&mut cx))).unwrap_err();
    println!("{}",payload.downcast_ref::<&str>().unwrap());
    let mut failed=Box::pin(body_panic());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||failed.as_mut().poll(&mut cx))).is_err());
    let payload=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||failed.as_mut().poll(&mut cx))).unwrap_err();
    println!("{}",payload.downcast_ref::<&str>().unwrap());
}
"#;
    std::fs::write(&path, format!("{SOURCE}\n{driver}")).unwrap();
    let compiled = std::process::Command::new("rustc")
        .args(["--edition=2024", "-Cpanic=unwind", "-o"])
        .arg(&binary)
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(&binary).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let messages = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 2);
    std::fs::remove_dir_all(root).unwrap();
    messages
}
