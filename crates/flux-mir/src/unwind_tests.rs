//! Exceptional control flow must preserve ownership at the call boundary.
use super::*;
use crate::init::{MovePaths, State, analyze};
use flux_hir::flib::{Decode, Decoder, Encode, Encoder};

fn terminator(kind: TerminatorKind, unwind: UnwindAction) -> Terminator {
    Terminator {
        kind,
        unwind,
        range: TextRange::default(),
    }
}

fn push(body: &mut Body, term: Terminator) -> BasicBlock {
    body.blocks.alloc(BasicBlockData {
        statements: Vec::new(),
        terminator: term,
    })
}

fn call_body(indirect: bool, returns: bool) -> Body {
    let mut body = Body::empty(None);
    for kind in [LocalKind::Return, LocalKind::Param, LocalKind::Temp] {
        body.locals.alloc(LocalDecl { ty: Ty::UNIT, kind });
    }
    body.param_count = 1;
    let args = vec![Operand::Move(Local::from_raw(1).into())];
    let dest = Local::from_raw(2).into();
    let target = returns.then_some(BasicBlock::from_raw(1));
    let kind = if indirect {
        TerminatorKind::CallIndirect {
            callee: Operand::Const(Constant::Unit),
            args,
            dest,
            target,
        }
    } else {
        TerminatorKind::Call {
            func: FunctionId::from_raw(0),
            generic_args: &[],
            args,
            dest,
            target,
        }
    };
    push(
        &mut body,
        terminator(kind, UnwindAction::Cleanup(BasicBlock::from_raw(2))),
    );
    push(
        &mut body,
        terminator(TerminatorKind::Return, UnwindAction::Unreachable),
    );
    push(
        &mut body,
        terminator(TerminatorKind::UnwindResume, UnwindAction::Unreachable),
    );
    body
}

#[test]
fn calls_move_arguments_on_both_edges_and_initialize_results_only_on_return() {
    for indirect in [false, true] {
        for returns in [false, true] {
            let body = call_body(indirect, returns);
            let paths = MovePaths::new(&body, &Signatures::default(), |_| true);
            let entry = analyze(&body, &paths, true);
            let arg = paths.find(&Local::from_raw(1).into()).unwrap().0;
            let dest = paths.find(&Local::from_raw(2).into()).unwrap().0;
            let mut state = State::new(paths.paths.len());
            assert!(
                entry.load(2, &mut state),
                "a divergent call must still reach cleanup"
            );
            assert!(!state.maybe_held[arg], "callee owns its moved argument");
            assert!(
                !state.maybe_held[dest],
                "panicking call did not produce a result"
            );
            assert!(state.maybe_uninit[dest]);
            assert_eq!(entry.load(1, &mut state), returns);
            if returns {
                assert!(!state.maybe_held[arg]);
                assert!(state.maybe_held[dest]);
                assert!(!state.maybe_uninit[dest]);
            }
            assert!(validate::unwind_errors(&body).is_empty());
        }
    }
}

#[test]
fn unwinding_preserves_the_previous_destination_value() {
    let mut body = call_body(false, true);
    body.blocks[Body::entry()].statements.push(Statement {
        kind: StatementKind::Assign(
            Local::from_raw(2).into(),
            Rvalue::Use(Operand::Const(Constant::Unit)),
        ),
        range: TextRange::default(),
    });
    let paths = MovePaths::new(&body, &Signatures::default(), |_| true);
    let entries = analyze(&body, &paths, true);
    let dest = paths.find(&Local::from_raw(2).into()).unwrap().0;
    let mut state = State::new(paths.paths.len());
    assert!(entries.load(2, &mut state));
    assert!(
        state.maybe_held[dest],
        "only a successful call overwrites its destination"
    );
}

#[test]
fn drop_consumes_ownership_before_entering_cleanup() {
    let mut body = call_body(false, true);
    body.blocks[Body::entry()].terminator.kind = TerminatorKind::Drop {
        place: Local::from_raw(1).into(),
        target: BasicBlock::from_raw(1),
    };
    let paths = MovePaths::new(&body, &Signatures::default(), |_| true);
    let entries = analyze(&body, &paths, true);
    let arg = paths.find(&Local::from_raw(1).into()).unwrap().0;
    let mut state = State::new(paths.paths.len());
    for block in [1, 2] {
        assert!(entries.load(block, &mut state));
        assert!(
            !state.maybe_held[arg],
            "panicking destructor must not be called twice"
        );
    }
}

#[test]
fn exceptional_successor_is_last_even_when_equal_to_normal_successor() {
    let mut body = call_body(true, true);
    let term = &mut body.blocks[Body::entry()].terminator;
    term.unwind = UnwindAction::Cleanup(BasicBlock::from_raw(1));
    assert_eq!(
        term.successors().collect::<Vec<_>>(),
        [BasicBlock::from_raw(1); 2]
    );
    assert!(
        !validate::unwind_errors(&body).is_empty(),
        "a cleanup target cannot be a normal return"
    );
}

#[test]
fn cleanup_rejects_returns_normal_entries_and_second_unwinds() {
    let body = call_body(false, true);
    for action in [
        UnwindAction::Continue,
        UnwindAction::Cleanup(BasicBlock::from_raw(2)),
    ] {
        let mut bad = body.clone();
        bad.blocks[BasicBlock::from_raw(2)].terminator = terminator(
            TerminatorKind::CallIndirect {
                callee: Operand::Const(Constant::Unit),
                args: Vec::new(),
                dest: Body::RETURN.into(),
                target: None,
            },
            action,
        );
        assert!(
            validate::unwind_errors(&bad)
                .iter()
                .any(|e| e.contains("instead of terminating"))
        );
    }
    let mut bad = body.clone();
    bad.blocks[BasicBlock::from_raw(2)].terminator.kind = TerminatorKind::Return;
    assert!(
        validate::unwind_errors(&bad)
            .iter()
            .any(|e| e.contains("cleanup returns"))
    );
    let mut bad = body.clone();
    bad.blocks[Body::entry()].terminator.unwind = UnwindAction::Cleanup(Body::entry());
    assert!(
        validate::unwind_errors(&bad)
            .iter()
            .any(|e| e.contains("normal control flow"))
    );
    let mut bad = body.clone();
    bad.blocks[Body::entry()].terminator.unwind =
        UnwindAction::Cleanup(BasicBlock::from_raw(u32::MAX));
    assert!(
        validate::unwind_errors(&bad)
            .iter()
            .any(|e| e.contains("invalid successor"))
    );
    let mut bad = body;
    bad.blocks[Body::entry()].terminator.unwind = UnwindAction::Unreachable;
    assert!(
        validate::unwind_errors(&bad)
            .iter()
            .any(|e| e.contains("resume outside cleanup"))
    );
}

#[test]
fn unwind_terminators_round_trip_without_changing_abort_metadata() {
    for action in [
        UnwindAction::Unreachable,
        UnwindAction::Continue,
        UnwindAction::Cleanup(BasicBlock::from_raw(2)),
        UnwindAction::Terminate,
    ] {
        let mut body = call_body(false, true);
        let term = &mut body.blocks[Body::entry()].terminator;
        term.unwind = action;
        let mut e = Encoder::new();
        term.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        if action == UnwindAction::Unreachable {
            let mut old = Encoder::new();
            term.kind.encode(&mut old).unwrap();
            term.range.encode(&mut old).unwrap();
            assert_eq!(
                bytes,
                old.into_bytes(),
                "abort-mode wire bytes must stay unchanged"
            );
        }
        let mut decoder = Decoder::new(&bytes);
        assert_eq!(Terminator::decode(&mut decoder).unwrap(), *term);
        decoder.finish().unwrap();
    }
    let body = call_body(false, true);
    let mut e = Encoder::new();
    body.encode(&mut e).unwrap();
    let bytes = e.into_bytes();
    let mut d = Decoder::new(&bytes);
    let copy = Body::decode(&mut d).unwrap();
    d.finish().unwrap();
    assert!(validate::unwind_errors(&copy).is_empty());
    for (a, b) in body.blocks.values().zip(copy.blocks.values()) {
        assert_eq!(a, b);
    }
    let mut bad = body;
    bad.blocks[Body::entry()].terminator.unwind = UnwindAction::Cleanup(BasicBlock::from_raw(99));
    let mut e = Encoder::new();
    bad.encode(&mut e).unwrap();
    assert!(Body::decode(&mut Decoder::new(&e.into_bytes())).is_err());
}
