//! The values of the constants array lengths name (`[u8; N]`), which
//! type checking needs before it infers the bodies that use them, and
//! which evaluating needs MIR: so [`check_file`] gives type checking a
//! function that infers the constants and the `const fn`s they call,
//! builds their MIR and evaluates them, as rustc's queries do.

use flux_arena::ArenaMap;
use flux_hir::{ConstId, Diagnostic, FunctionId, Hir};
use flux_ty::{Signatures, TypedFile};

/// Type-checks `hir`, evaluating the constants array lengths name.
pub fn check_file(hir: &Hir) -> TypedFile {
    flux_ty::check_file_with(hir, &eval_lens)
}

/// The values of `wanted`, each `None` if its evaluation failed (the
/// diagnostics say why) or its value cannot be known, given `sigs`.
/// A constant whose own array lengths name others not known yet waits
/// for them, which are evaluated too; one that waits for itself is a
/// cycle.
fn eval_lens(
    hir: &Hir,
    sigs: &Signatures,
    wanted: &[ConstId],
) -> (Vec<(ConstId, Option<u64>)>, Vec<Diagnostic>) {
    let items = &hir.items;
    let mut sigs = sigs.clone();
    let mut done: Vec<(ConstId, Option<u64>)> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut pending: Vec<ConstId> = wanted.to_vec();
    while !pending.is_empty() {
        let typed = infer_consts(hir, sigs.clone());
        let clean = typed.consts.iter().all(|(_, r)| r.diagnostics.is_empty())
            && typed.bodies.iter().all(|(_, r)| r.diagnostics.is_empty());
        let mut consts = ArenaMap::new();
        for (id, result) in typed.consts.iter() {
            if result.diagnostics.is_empty() && result.missing_lens.is_empty() {
                consts.insert(id, crate::build::build_const(hir, &typed, id));
            }
        }
        let mut bodies = ArenaMap::new();
        for (id, result) in typed.bodies.iter() {
            if result.diagnostics.is_empty() && result.missing_lens.is_empty() {
                bodies.insert(id, crate::build::build(hir, &typed, id));
            }
        }
        // Code type checking should have ruled out is there for errors
        // elsewhere (a name whose import failed), which evaluation would
        // report as reaching `unreachable`, where rustc reports nothing.
        let clean = clean
            && (consts.iter().map(|(_, b)| b))
                .chain(bodies.iter().map(|(_, b)| b))
                .all(|b| b.bugs.is_empty());
        let names = |id: ConstId| items.const_name(id);
        let mut evaluator =
            crate::eval::Evaluator::new(items, &typed.sigs, &bodies, &consts, &names);
        let mut waiting = Vec::new();
        let mut progress = false;
        for &c in &pending {
            let missing = typed.consts.get(c).map_or(&[][..], |r| &r.missing_lens[..]);
            if !missing.is_empty() {
                for &m in missing {
                    if !pending.contains(&m) && !waiting.contains(&m) {
                        waiting.push(m);
                    }
                }
                waiting.push(c);
                continue;
            }
            progress = true;
            let value = match evaluator.eval_const(c) {
                Ok(crate::eval::Value::Int { bits, .. }) => u64::try_from(bits).ok(),
                _ => None,
            };
            done.push((c, value));
            sigs.array_lens.insert(c, value);
        }
        if clean {
            for d in crate::build::eval_failures(items, &evaluator) {
                if !diagnostics.contains(&d) {
                    diagnostics.push(d);
                }
            }
        }
        waiting.dedup();
        if !progress && waiting.iter().all(|w| pending.contains(w)) {
            // Each waits for another that waits: a cycle.
            for &c in &waiting {
                let data = &items.consts[c];
                let mut d = Diagnostic::new(
                    data.def_range,
                    format!(
                        "cycle detected when evaluating type-level constant `{}`",
                        items.const_name(c)
                    ),
                );
                d.file = data.loc.file;
                diagnostics.push(d);
                done.push((c, None));
            }
            break;
        }
        pending = waiting;
    }
    (done, diagnostics)
}

/// Every constant's value inferred against `sigs`, and the functions
/// they may call.
fn infer_consts(hir: &Hir, sigs: Signatures) -> TypedFile {
    let items = &hir.items;
    let mut queue: Vec<FunctionId> = Vec::new();
    let reached = |result: &flux_ty::InferenceResult, queue: &mut Vec<FunctionId>| {
        let called = result.calls.iter().map(|(_, i)| i.function);
        let methods = result.methods.iter().map(|(_, m)| m.function);
        let operators = result.overloads.iter().map(|(_, i)| i.function);
        for f in called.chain(methods).chain(operators) {
            queue.extend(flux_ty::call_targets(items, f));
        }
    };
    let mut consts = ArenaMap::new();
    for (id, (body, source_map)) in hir.const_bodies() {
        let result = flux_ty::infer_const(items, &sigs, id, body, source_map);
        reached(&result, &mut queue);
        consts.insert(id, result);
    }
    let mut bodies = ArenaMap::new();
    while let Some(id) = queue.pop() {
        if bodies.get(id).is_some() || !items.functions[id].is_const {
            continue;
        }
        let Some((body, source_map)) = hir.body(id) else {
            continue;
        };
        let result = flux_ty::infer_body(items, &sigs, id, body, source_map);
        reached(&result, &mut queue);
        bodies.insert(id, result);
    }
    TypedFile {
        sigs,
        bodies,
        consts,
        diagnostics: Vec::new(),
    }
}
