//! MIR-backed Send/Sync facts for the storage owned by source coroutines.

use crate::imported::{ImportedMir, SharedMir};
use crate::{Body, TerminatorKind};
use flux_hir::{Diagnostic, FunctionId, Hir, Intrinsic, ItemTree};
use flux_ty::{
    ClosureArgs, ClosureId, ClosureKind, ClosureOwner, GenericArgs, Ty, TyKind, TypedFile,
};
use rustc_hash::{FxHashMap, FxHashSet};

/// Complete local obligations before the typed file can be built or exported.
/// The checked templates already present in dependency MIR are also a source
/// of facts for later monomorphized impl selection, without a format change.
pub(crate) fn fulfill(
    hir: &Hir,
    typed: &mut TypedFile,
    imported: Option<&ImportedMir>,
    required: &[FunctionId],
) {
    if !typed.diagnostics.is_empty() {
        return;
    }
    let mut owners = Vec::new();
    let mut goals = Vec::new();
    for (id, result) in typed.bodies.iter() {
        if local_async_owner(hir, ClosureOwner::Function(id), result) {
            owners.push(ClosureOwner::Function(id));
        }
        goals.extend(
            result
                .deferred_auto_traits
                .iter()
                .map(|goal| goal.predicate.self_ty),
        );
    }
    for (id, result) in typed.consts.iter() {
        if local_async_owner(hir, ClosureOwner::Const(id), result) {
            owners.push(ClosureOwner::Const(id));
        }
        goals.extend(
            result
                .deferred_auto_traits
                .iter()
                .map(|goal| goal.predicate.self_ty),
        );
    }
    if let Some(imported) = imported {
        // One borrowed scan avoids searching every closure once per caller.
        let available = imported.coroutine_owners(&hir.items);
        if !available.is_empty() {
            let mut requested = required.to_vec();
            for result in typed
                .bodies
                .iter()
                .map(|(_, result)| result)
                .chain(typed.consts.iter().map(|(_, result)| result))
            {
                requested.extend(result.calls.iter().map(|(_, instance)| instance.function));
                requested.extend(result.methods.iter().map(|(_, method)| method.function));
                requested.extend(
                    result
                        .overloads
                        .iter()
                        .map(|(_, instance)| instance.function),
                );
            }
            requested.sort_unstable_by_key(|id| id.into_raw());
            requested.dedup();
            for id in requested {
                let owner = ClosureOwner::Function(id);
                if available.contains(&owner) {
                    owners.push(owner);
                }
            }
        }
    }
    if owners.is_empty() && goals.is_empty() {
        // A pending flag without its proof obligations is not a valid state.
        if typed.sigs.pending_auto_trait_proofs {
            typed.diagnostics.push(Diagnostic::new(
                Default::default(),
                "coroutine auto-trait proofs are unresolved",
            ));
        }
        return;
    }
    let facts = collect(hir, typed, imported, owners, goals);
    typed.sigs.coroutine_witnesses.extend(facts);
    let sigs = &typed.sigs;
    let mut diagnostics = Vec::new();
    let functions: Vec<_> = typed.bodies.iter().map(|(id, _)| id).collect();
    for id in functions {
        let result = typed.bodies.get_mut(id).expect("collected function result");
        discharge(
            hir,
            sigs,
            &mut result.deferred_auto_traits,
            hir.items.functions[id].loc.file,
            &mut diagnostics,
        );
    }
    let constants: Vec<_> = typed.consts.iter().map(|(id, _)| id).collect();
    for id in constants {
        let result = typed.consts.get_mut(id).expect("collected constant result");
        discharge(
            hir,
            sigs,
            &mut result.deferred_auto_traits,
            hir.items.consts[id].loc.file,
            &mut diagnostics,
        );
    }
    typed.sigs.pending_auto_trait_proofs = typed
        .bodies
        .iter()
        .map(|(_, result)| result)
        .chain(typed.consts.iter().map(|(_, result)| result))
        .any(|result| !result.deferred_auto_traits.is_empty());
    typed.diagnostics.extend(diagnostics);
    Diagnostic::sort(&mut typed.diagnostics);
}

fn discharge(
    hir: &Hir,
    sigs: &flux_ty::Signatures,
    goals: &mut Vec<flux_ty::DeferredAutoTrait>,
    file: flux_hir::FileId,
    diagnostics: &mut Vec<Diagnostic>,
) {
    goals.retain(|goal| {
        let solver = flux_solve::traits::RigidSolver::new(
            &hir.items,
            sigs,
            &goal.caller_bounds,
            &goal.caller_projections,
            &goal.unsized_params,
        );
        let holds = !goal.predicate.self_ty.has_error()
            && !goal.predicate.self_ty.has_infer()
            && solver.holds(goal.predicate, &mut Vec::new());
        if !holds {
            diagnostics.push(Diagnostic {
                file,
                range: goal.range,
                message: format!("the trait bound `{}` is not satisfied", goal.root),
            });
        }
        !holds
    });
}

fn local_async_owner(hir: &Hir, owner: ClosureOwner, result: &flux_ty::InferenceResult) -> bool {
    let source = match owner {
        ClosureOwner::Function(id) => hir.body(id),
        ClosureOwner::Const(id) => hir.const_body(id),
    };
    source.is_some_and(|(body, _)| {
        result.closures.iter().any(|(expr, _)| {
            matches!(
                body.exprs[expr],
                flux_hir::Expr::Closure { is_async: true, .. }
            )
        })
    })
}

pub(crate) fn constructs_coroutine(items: &ItemTree, body: &Body) -> bool {
    body.blocks.values().any(|block| matches!(block.terminator.kind,
        TerminatorKind::Call { func, .. } if items.functions[func].intrinsic == Some(Intrinsic::CoroutineNew)))
}

fn collect(
    hir: &Hir,
    typed: &TypedFile,
    imported: Option<&ImportedMir>,
    mut owners: Vec<ClosureOwner>,
    seeds: Vec<Ty>,
) -> FxHashMap<ClosureId, GenericArgs> {
    let mut facts = FxHashMap::default();
    let mut built_owners = FxHashSet::default();
    let mut seen = FxHashSet::default();
    let mut pending: Vec<_> = seeds.into_iter().map(|ty| (ty, 0u32)).collect();
    // Missing facts remain unknown. Limits never manufacture an empty witness.
    let mut budget = 100_000usize;
    while (!owners.is_empty() || !pending.is_empty()) && budget != 0 {
        budget -= 1;
        if let Some(owner) = owners.pop() {
            if !built_owners.insert(owner) {
                continue;
            }
            let built = if let Some(provider) = SharedMir::of(hir, imported, owner) {
                provider.built(owner)
            } else {
                let result = match owner {
                    ClosureOwner::Function(id) => typed.bodies.get(id),
                    ClosureOwner::Const(id) => typed.consts.get(id),
                };
                result
                    .filter(|result| {
                        result.diagnostics.is_empty() && result.missing_lens.is_empty()
                    })
                    .map(|_| match owner {
                        ClosureOwner::Function(id) => crate::build::Built::function(
                            hir,
                            typed,
                            id,
                            crate::PanicStrategy::Abort,
                        ),
                        ClosureOwner::Const(id) => {
                            crate::build::Built::konst(hir, typed, id, crate::PanicStrategy::Abort)
                        }
                    })
            };
            let Some(built) = built else {
                continue;
            };
            // An owner may contain nested async blocks. Their own template
            // captures and awaited types can name further imported owners.
            for closure in built.closures {
                if closure.kind != ClosureKind::FnOnce {
                    continue;
                }
                if let Some(storage) = suspension_storage(hir, &typed.sigs, &closure.body) {
                    pending.extend(storage.iter().map(|&ty| (ty, 0)));
                    facts.insert(closure.id, Ty::list(&storage));
                }
            }
            continue;
        }
        let Some((ty, depth)) = pending.pop() else {
            continue;
        };
        if depth > 128 || !seen.insert(ty) {
            continue;
        }
        let mut next = |ty| pending.push((ty, depth + 1));
        match ty.kind() {
            TyKind::Opaque(opaque) => {
                if let Some(hidden) = typed.sigs.hidden.get(&(opaque.function, opaque.index)) {
                    next(hidden.subst(opaque.args));
                }
            }
            TyKind::Alias(_) => {
                let normalized = flux_solve::traits::normalize(&hir.items, &typed.sigs, &[], ty);
                if normalized != ty {
                    next(normalized);
                }
            }
            TyKind::Adt(adt, args)
                if flux_solve::is_lang_struct(&hir.items, adt.strukt, "coroutine") =>
            {
                if let Some(closure) = args.first()
                    && let TyKind::Closure(id, closure_args) = closure.kind()
                {
                    let parts = ClosureArgs::split(closure_args);
                    next(parts.upvars);
                    if let Some(stored) = facts
                        .get(&id)
                        .or_else(|| typed.sigs.coroutine_witnesses.get(&id))
                    {
                        for &ty in *stored {
                            next(ty.subst(parts.parent));
                        }
                    } else if !built_owners.contains(&id.owner) {
                        // Retry the concrete request after its owner is built.
                        seen.remove(&ty);
                        next(ty);
                        owners.push(id.owner);
                    }
                }
            }
            TyKind::Adt(adt, args) => {
                for &field in flux_solve::field_types(&typed.sigs, adt.strukt, args) {
                    next(field);
                }
            }
            _ => {
                for child in ty.children() {
                    next(child);
                }
            }
        }
    }
    facts
}

/// The local storage that is initialized while an await can return Pending.
/// live_OUT includes the normal successor's result, so intersect it with the
/// state immediately BEFORE the call assigns its output. Captures are required
/// independently by the solver; they exist even before the first poll.
pub(crate) fn suspension_storage(
    hir: &Hir,
    sigs: &flux_ty::Signatures,
    body: &Body,
) -> Option<Vec<Ty>> {
    if !body.bugs.is_empty() {
        return None;
    }
    let tracked = vec![true; body.locals.len()];
    let held = crate::borrowck::held_before_terminators(body, sigs, &tracked);
    let live_out = crate::borrowck::liveness_where(body, &tracked, true, None);
    let mut storage = Vec::new();
    for (block, data) in body.blocks.iter() {
        let TerminatorKind::Call { func, args, .. } = &data.terminator.kind else {
            continue;
        };
        if hir.items.functions[*func].intrinsic != Some(Intrinsic::CoroutineAwait) {
            continue;
        }
        // The awaited future is used by this terminator, even when it has no
        // later source use and consequently does not appear in live_OUT.
        let TyKind::Ref(_, future) = body.operand_ty(args.first()?, sigs).kind() else {
            return None;
        };
        storage.push(future);
        for &local in &live_out[block.index()] {
            if held[block.index()].contains(local as usize) {
                storage.push(body.locals[crate::Local::from_raw(local)].ty);
            }
        }
    }
    if storage.iter().any(|ty| ty.has_infer() || ty.has_error()) {
        return None;
    }
    // Whole-local types conservatively retain initialized siblings after a
    // partial move. Do not infer absence merely from an uninitialized parent.
    let mut seen = FxHashSet::default();
    storage.retain(|ty| seen.insert(*ty));
    Some(storage)
}
