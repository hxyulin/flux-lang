//! Persistence requires every built-in body to pass, including unreachable code.

use flux_hir::{Core, Diagnostic, SourceDb};

fn assert_clean(stage: &str, db: &SourceDb, diagnostics: &[Diagnostic]) {
    assert!(
        diagnostics.is_empty(),
        "{stage}:\n{}",
        diagnostics
            .iter()
            .map(|d| {
                let path = db
                    .files()
                    .find(|(id, _)| *id == d.file)
                    .map(|(_, file)| file.path.display().to_string())
                    .unwrap_or_else(|| "<unknown source>".to_owned());
                format!("{path}:{:?}: {}", d.range, d.message)
            })
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn every_builtin_function_and_constant_passes_the_complete_mir_pipeline() {
    let db = SourceDb::new();
    assert_clean("syntax", &db, &db.syntax_errors());
    let core = Core::get();
    let hir = core.hir();
    assert_clean("item collection", &db, &hir.diagnostics);
    let functions: Vec<_> = hir
        .items
        .functions
        .iter()
        .filter(|(_, data)| data.has_body)
        .map(|(id, _)| id)
        .collect();
    let constants: Vec<_> = hir
        .items
        .consts
        .iter()
        .filter(|(_, data)| data.has_body)
        .map(|(id, _)| id)
        .collect();
    assert!(!functions.is_empty());
    assert!(!constants.is_empty());
    for &id in &functions {
        let lowered = core.lower_body(id);
        assert_clean(
            &format!(
                "lower {}",
                hir.items.function_path(&hir.items.functions[id])
            ),
            &db,
            &lowered.diagnostics,
        );
    }
    let typed = flux_ty::check_file_all(hir);
    assert_clean("type checking", &db, &typed.diagnostics);
    for (id, data) in hir.items.functions.iter() {
        assert!(
            typed.sigs.fns.get(id).is_some(),
            "missing signature for {}",
            hir.items.function_path(data)
        );
    }
    for (id, _) in hir.items.consts.iter() {
        assert!(
            typed.sigs.consts.get(id).is_some(),
            "missing constant signature for {}",
            hir.items.const_name(id)
        );
    }
    assert_eq!(typed.bodies.iter().count(), functions.len());
    assert_eq!(typed.consts.iter().count(), constants.len());
    for &id in &functions {
        assert!(
            typed.bodies.get(id).is_some(),
            "missing inference for {}",
            hir.items.function_path(&hir.items.functions[id])
        );
    }
    for &id in &constants {
        assert!(
            typed.consts.get(id).is_some(),
            "missing constant inference for {}",
            hir.items.const_name(id)
        );
    }
    let mir = flux_mir::build_file_with_roots(hir, &typed, &functions);
    assert_clean("MIR checks", &db, &mir.diagnostics);
    assert_clean("MIR internal errors", &db, &mir.bugs);
    for &id in &functions {
        assert!(
            mir.bodies.get(id).is_some(),
            "missing MIR for {}",
            hir.items.function_path(&hir.items.functions[id])
        );
    }
    for &id in &constants {
        assert!(
            mir.consts.get(id).is_some(),
            "missing constant MIR for {}",
            hir.items.const_name(id)
        );
    }
    assert_eq!(mir.bodies.iter().count(), functions.len());
    assert_eq!(mir.consts.iter().count(), constants.len());
    let mut closure_count = 0;
    for result in typed
        .bodies
        .iter()
        .map(|(_, result)| result)
        .chain(typed.consts.iter().map(|(_, result)| result))
    {
        for (_, closure) in result.closures.iter() {
            let flux_ty::TyKind::Closure(id, _) = closure.ty.kind() else {
                panic!("closure inference must have a closure type")
            };
            let kinds: &[flux_ty::ClosureKind] = match closure.kind {
                flux_ty::ClosureKind::Fn => &[
                    flux_ty::ClosureKind::Fn,
                    flux_ty::ClosureKind::FnMut,
                    flux_ty::ClosureKind::FnOnce,
                ],
                flux_ty::ClosureKind::FnMut => {
                    &[flux_ty::ClosureKind::FnMut, flux_ty::ClosureKind::FnOnce]
                }
                flux_ty::ClosureKind::FnOnce => &[flux_ty::ClosureKind::FnOnce],
            };
            for &kind in kinds {
                assert!(
                    mir.closures.contains_key(&(id, kind)),
                    "missing {kind:?} closure body/shim for {id:?}"
                );
                closure_count += 1;
            }
        }
    }
    assert_eq!(mir.closures.len(), closure_count);
    let errors = flux_mir::validate::validate_file(hir, &typed.sigs, &mir);
    assert!(errors.is_empty(), "MIR validation:\n{}", errors.join("\n"));
    eprintln!(
        "eager built-in coverage: {} functions, {} constants, {} closure bodies/shims",
        functions.len(),
        constants.len(),
        mir.closures.len()
    );
}
