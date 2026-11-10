//! Exceptional MIR must not silently become ordinary calls.
use flux_mir::{TerminatorKind, UnwindAction};

#[test]
fn exceptional_mir_is_rejected_in_abort_compilations() {
    let source = "fn called() {} fn main() { called(); }";
    let (_, _, hir) = flux_hir::lower_text("unwind.rs", source);
    assert!(hir.diagnostics.is_empty());
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty());
    let mut mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty());
    flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let (owner, block) = mir
        .bodies
        .iter()
        .filter(|(id, _)| hir.is_program(*id))
        .find_map(|(id, body)| {
            body.blocks
                .iter()
                .find(|(_, data)| matches!(data.terminator.kind, TerminatorKind::Call { .. }))
                .map(|(block, _)| (id, block))
        })
        .expect("program has a direct call");
    for action in [UnwindAction::Continue, UnwindAction::Terminate] {
        mir.bodies.get_mut(owner).unwrap().blocks[block]
            .terminator
            .unwind = action;
        let errors = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message == "exceptional MIR requires panic=unwind")
        );
    }
    mir.bodies.get_mut(owner).unwrap().blocks[block]
        .terminator
        .unwind = UnwindAction::Unreachable;
    mir.bodies.get_mut(owner).unwrap().blocks[block]
        .terminator
        .kind = TerminatorKind::UnwindResume;
    let errors = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message == "exceptional MIR requires panic=unwind")
    );
    let built =
        flux_mir::build_with_panic_strategy(&hir, &typed, owner, flux_mir::PanicStrategy::Unwind);
    assert!(built.has_unwind());
    mir.bodies.insert(owner, built);
    let errors = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message == "exceptional MIR requires panic=unwind")
    );
    mir.bodies.get_mut(owner).unwrap().blocks[block]
        .terminator
        .kind = TerminatorKind::UnwindTerminate;
    let errors = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message == "exceptional MIR requires panic=unwind")
    );
}
