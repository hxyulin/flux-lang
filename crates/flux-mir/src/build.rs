//! Builds MIR from a type-checked HIR body.

use flux_arena::{Arena, ArenaMap};
use flux_hir::{
    ArithOp, BinaryOp, Binding, BindingId, CmpOp, ConstId, Expr, ExprId, FunctionId, Hir, ItemTree,
    Literal, LogicOp, MatchArm, Pat, PatId, Res, Stmt, StructId, UnaryOp,
};
use flux_intern::Symbol;
use flux_syntax::TextRange;
use flux_ty::{GenericArgs, InferenceResult, IntTy, Mutability, Ty, TyKind, TypedFile};

use crate::{
    AggregateKind, AssertMessage, BasicBlock, BasicBlockData, BinOp, Body, Constant, FileMir,
    Local, LocalDecl, LocalKind, Operand, Place, PlaceTy, ProjectionElem, Rvalue, Statement,
    StatementKind, Terminator, TerminatorKind, UnOp,
};

/// Builds every body of a file and checks initialization. The file must
/// have no diagnostics from lowering or type checking.
pub fn build_file(hir: &Hir, typed: &TypedFile) -> FileMir {
    let mut diagnostics = Vec::new();
    let none = crate::borrowck::Requirements::default();
    let check = |body: &Body, file, is_const, requirements: &crate::borrowck::Requirements| {
        let (found, own) = check_body(hir, typed, body, is_const, requirements);
        let found: Vec<_> = found
            .into_iter()
            .map(move |d| flux_hir::Diagnostic { file, ..d })
            .collect();
        (found, own)
    };
    // Every constant, and every function of the program; `core`'s
    // functions only if something calls them.
    let mut queue: Vec<FunctionId> = Vec::new();
    let mut consts = ArenaMap::new();
    let mut unchecked = Vec::new();
    for (id, _) in hir.const_bodies() {
        let body = build_const(hir, typed, id);
        let found = check(&body, hir.items.consts[id].loc.file, true, &none).0;
        if !found.is_empty() {
            unchecked.push(id);
        }
        diagnostics.extend(found);
        queue.extend(body.callees());
        consts.insert(id, body);
    }
    queue.extend(flux_ty::core_roots(&hir.items));
    queue.extend(
        hir.bodies
            .iter()
            .map(|(id, _)| id)
            .filter(|&id| hir.is_program(id)),
    );
    let mut bodies = ArenaMap::new();
    // The callees whose targets are queued already.
    let mut expanded = rustc_hash::FxHashSet::default();
    let mut closures = std::collections::HashMap::new();
    while let Some(id) = queue.pop() {
        if bodies.get(id).is_some() || hir.body(id).is_none() || typed.bodies.get(id).is_none() {
            continue;
        }
        let body = build(hir, typed, id);
        for callee in body.callees() {
            if expanded.insert(callee) {
                queue.extend(flux_ty::call_targets(&hir.items, callee));
            }
        }
        // Its closures, innermost first, and what they call.
        let mut found: Vec<ExprId> = typed.bodies[id].closures.iter().map(|(e, _)| e).collect();
        found.sort_by_key(|e| e.into_raw());
        for expr in found {
            let closure = build_closure(hir, typed, id, expr);
            for callee in closure.callees() {
                queue.extend(flux_ty::call_targets(&hir.items, callee));
            }
            let kind = typed.bodies[id].closures[expr].kind;
            let key = flux_ty::ClosureId { owner: id, expr };
            closures.insert((key, kind), closure);
            for (mode, shim) in closure_shims(hir, typed, id, expr) {
                closures.insert((key, mode), shim);
            }
        }
        bodies.insert(id, body);
    }
    // Code that type checking should have ruled out, in a program
    // without errors: a bug in flux, which must not compile to a trap.
    let bugs = bugs(hir, &bodies, &closures, &consts);
    assert!(
        !cfg!(debug_assertions) || bugs.is_empty(),
        "MIR building: {}",
        (bugs.iter())
            .map(|d| format!("{} at {:?}", d.message, d.range))
            .collect::<Vec<_>>()
            .join("; ")
    );
    diagnostics.extend(bugs.iter().cloned());
    // Patterns that name constants are checked with their values; rustc
    // does not borrow-check the bodies whose patterns have errors.
    diagnostics.extend(crate::borrowck::check_impl_methods(&hir.items, &typed.sigs));
    let (found, failed) = check_patterns(hir, typed, &bodies, &consts, &unchecked, true);
    diagnostics.extend(found);
    for (id, body) in bodies.iter() {
        let data = &hir.items.functions[id];
        if !failed.contains(&id) {
            // Its closures' own bodies first, innermost first, for what
            // they require of the bodies that make them; shims need no
            // checks.
            let mut own: Vec<_> = closures
                .iter()
                .filter(|((c, kind), _)| {
                    c.owner == id && typed.bodies[id].closures[c.expr].kind == *kind
                })
                .collect();
            own.sort_by_key(|((c, _), _)| c.expr.into_raw());
            let mut requirements = crate::borrowck::Requirements::default();
            let mut found = Vec::new();
            for ((c, _), closure) in own {
                let (errors, required) = check(closure, data.loc.file, false, &requirements);
                found.extend(errors);
                requirements.insert(*c, required);
            }
            diagnostics.extend(check(body, data.loc.file, data.is_const, &requirements).0);
            diagnostics.extend(found);
        }
    }
    let mut const_values = crate::eval::ConstValues::default();
    let mut discriminants = crate::Discriminants::new();
    if diagnostics.is_empty() {
        diagnostics.extend(evaluate_consts(
            hir,
            typed,
            &bodies,
            &consts,
            &mut const_values,
            &mut discriminants,
        ));
        // rustc lints associated constants even where they are unused.
        let linted = bodies
            .iter()
            .map(|(id, body)| (hir.items.functions[id].loc.file, body))
            .chain(
                closures
                    .iter()
                    .map(|((c, _), body)| (hir.items.functions[c.owner].loc.file, body)),
            )
            .chain(consts.iter().filter_map(|(id, body)| {
                let data = &hir.items.consts[id];
                data.owner.map(|_| (data.loc.file, body))
            }));
        for (file, body) in linted {
            let found = crate::lint::check(body, &typed.sigs, &const_values);
            diagnostics.extend(
                found
                    .into_iter()
                    .map(|d| flux_hir::Diagnostic { file, ..d }),
            );
        }
    }
    flux_hir::Diagnostic::sort(&mut diagnostics);
    let mir = FileMir {
        bodies,
        closures,
        consts,
        const_values,
        discriminants,
        diagnostics,
        bugs,
    };
    // Code generation trusts the types of places: check them.
    if cfg!(debug_assertions) {
        let errors = crate::validate::validate_file(hir, &typed.sigs, &mir);
        assert!(
            errors.is_empty(),
            "ill-typed MIR:\n  {}",
            errors.join("\n  ")
        );
    }
    mir
}

/// What building `bodies`, `closures` and `consts` lowered to
/// `unreachable` because type checking should have ruled it out, as
/// errors.
fn bugs(
    hir: &Hir,
    bodies: &ArenaMap<flux_hir::Function, Body>,
    closures: &std::collections::HashMap<(flux_ty::ClosureId, flux_ty::ClosureKind), Body>,
    consts: &ArenaMap<flux_hir::Const, Body>,
) -> Vec<flux_hir::Diagnostic> {
    let functions = bodies
        .iter()
        .map(|(id, body)| (hir.items.functions[id].loc.file, body));
    let closures =
        (closures.iter()).map(|((c, _), body)| (hir.items.functions[c.owner].loc.file, body));
    let consts = consts
        .iter()
        .map(|(id, body)| (hir.items.consts[id].loc.file, body));
    let mut found: Vec<flux_hir::Diagnostic> = functions
        .chain(closures)
        .chain(consts)
        .flat_map(|(file, body)| {
            body.bugs
                .iter()
                .map(move |&(range, what)| flux_hir::Diagnostic {
                    file,
                    ..flux_hir::Diagnostic::new(range, format!("internal compiler error: {what}"))
                })
        })
        .collect();
    flux_hir::Diagnostic::sort(&mut found);
    found.dedup();
    found
}

/// The initialization and borrow errors of the program's functions whose
/// signatures and bodies have none of `errors`, for when others do:
/// rustc borrow-checks every body that type-checked.
pub fn check_clean_bodies(
    hir: &Hir,
    typed: &TypedFile,
    errors: &[flux_hir::Diagnostic],
) -> Vec<flux_hir::Diagnostic> {
    let (mut diagnostics, failed) = check_clean_patterns(hir, typed);
    for (id, _) in hir.bodies.iter() {
        if failed.contains(&id) {
            continue;
        }
        let Some(result) = typed.bodies.get(id) else {
            continue;
        };
        let data = &hir.items.functions[id];
        let range = data.item_range;
        let tainted = errors.iter().any(|d| {
            d.file == data.loc.file
                && range.start() <= d.range.start()
                && d.range.end() <= range.end()
        });
        let sig = &typed.sigs[id];
        if !hir.is_program(id)
            || tainted
            || !result.diagnostics.is_empty()
            || sig.params.contains(&Ty::ERROR)
            || sig.ret == Ty::ERROR
        {
            continue;
        }
        let body = build(hir, typed, id);
        let file = data.loc.file;
        // Its closures first, for what they require of it.
        let mut found: Vec<ExprId> = result.closures.iter().map(|(e, _)| e).collect();
        found.sort_by_key(|e| e.into_raw());
        let mut requirements = crate::borrowck::Requirements::default();
        let mut errors = Vec::new();
        for expr in found {
            let closure = build_closure(hir, typed, id, expr);
            let (found, required) = check_body(hir, typed, &closure, false, &requirements);
            errors.extend(found);
            requirements.insert(flux_ty::ClosureId { owner: id, expr }, required);
        }
        let (found, _) = check_body(hir, typed, &body, data.is_const, &requirements);
        diagnostics.extend(
            found
                .into_iter()
                .chain(errors)
                .map(|d| flux_hir::Diagnostic { file, ..d }),
        );
    }
    diagnostics
}

/// [`check_patterns`] where other bodies have errors: with the values
/// of the constants that have none, and call only functions that have
/// none. Constants that fail are not reported.
fn check_clean_patterns(
    hir: &Hir,
    typed: &TypedFile,
) -> (Vec<flux_hir::Diagnostic>, Vec<FunctionId>) {
    if !has_deferred_patterns(typed) {
        return Default::default();
    }
    let clean = |result: Option<&InferenceResult>| result.is_some_and(|r| r.diagnostics.is_empty());
    let mut queue = Vec::new();
    let mut consts = ArenaMap::new();
    for (id, _) in hir.const_bodies() {
        if clean(typed.consts.get(id)) && typed.sigs.consts.get(id) != Some(&Ty::ERROR) {
            let body = build_const(hir, typed, id);
            queue.extend(body.callees());
            consts.insert(id, body);
        }
    }
    let mut bodies = ArenaMap::new();
    // The callees whose targets are queued already.
    let mut expanded = rustc_hash::FxHashSet::default();
    while let Some(id) = queue.pop() {
        if bodies.get(id).is_some() || hir.body(id).is_none() || !clean(typed.bodies.get(id)) {
            continue;
        }
        let body = build(hir, typed, id);
        for callee in body.callees() {
            if expanded.insert(callee) {
                queue.extend(flux_ty::call_targets(&hir.items, callee));
            }
        }
        bodies.insert(id, body);
    }
    check_patterns(hir, typed, &bodies, &consts, &[], false)
}

/// Whether some body's pattern checks wait for the values of
/// constants.
fn has_deferred_patterns(typed: &TypedFile) -> bool {
    let functions = typed.bodies.iter().map(|(_, r)| r);
    functions
        .chain(typed.consts.iter().map(|(_, r)| r))
        .any(|r| r.deferred_pats)
}

/// The checks of the patterns that name constants, with the values of
/// the constants, evaluated from `bodies` and `consts` but those in
/// `skip`; and the functions they fail in. With `report`, constants
/// whose evaluation fails are reported, as rustc does (E0080).
fn check_patterns(
    hir: &Hir,
    typed: &TypedFile,
    bodies: &ArenaMap<flux_hir::Function, Body>,
    consts: &ArenaMap<flux_hir::Const, Body>,
    skip: &[ConstId],
    report: bool,
) -> (Vec<flux_hir::Diagnostic>, Vec<FunctionId>) {
    if !has_deferred_patterns(typed) {
        return Default::default();
    }
    let items = &hir.items;
    let names = |id: ConstId| items.const_name(id);
    let mut evaluator = crate::eval::Evaluator::new(items, &typed.sigs, bodies, consts, &names);
    let (mut diagnostics, failed) = flux_ty::check_deferred_patterns(hir, typed, &mut |c| {
        if skip.contains(&c.konst) {
            return None;
        }
        match evaluator.eval_instance(c).ok()? {
            crate::eval::Value::Int { bits, ty } if ty.is_signed() => {
                Some(crate::eval::sign_extend(bits, ty))
            }
            crate::eval::Value::Int { bits, .. } => Some(bits as i128),
            crate::eval::Value::Bool(b) => Some(b as i128),
            _ => None,
        }
    });
    if report {
        diagnostics.extend(eval_failures(items, &evaluator));
    }
    (diagnostics, failed)
}

/// The constants whose evaluation failed, as rustc reports them: at the
/// constant, once each.
pub(crate) fn eval_failures(
    items: &flux_hir::ItemTree,
    evaluator: &crate::eval::Evaluator<'_>,
) -> Vec<flux_hir::Diagnostic> {
    let mut diagnostics: Vec<flux_hir::Diagnostic> = Vec::new();
    for (c, e) in &evaluator.failures {
        let mut d = flux_hir::Diagnostic::new(e.range, e.message.clone());
        d.file = items.consts[c.konst].loc.file;
        if !diagnostics.contains(&d) {
            diagnostics.push(d);
        }
    }
    diagnostics
}

/// A body's initialization and borrow errors, and if it is of a
/// constant or `const fn`, its drops that would run code.
fn check_body(
    hir: &Hir,
    typed: &TypedFile,
    body: &Body,
    is_const: bool,
    requirements: &crate::borrowck::Requirements,
) -> (
    Vec<flux_hir::Diagnostic>,
    crate::borrowck::ClosureRequirements,
) {
    let mut found = body.init_errors.clone();
    let (errors, required) = crate::borrowck::check(body, &hir.items, &typed.sigs, requirements);
    found.extend(errors);
    if is_const {
        found.extend(crate::drops::check_const(body));
    }
    (found, required)
}

/// Evaluates the constants rustc evaluates: enums' discriminants
/// first, as rustc checks enums before evaluating constants, then free
/// constants, and those that code without generic parameters names,
/// with the arguments it gives, whether it is used or not. Errors are
/// E0080, at the value of the constant that failed. The other constants
/// without generic parameters are evaluated for their values only;
/// those with some, for the instances code generation needs.
fn evaluate_consts(
    hir: &Hir,
    typed: &TypedFile,
    bodies: &ArenaMap<flux_hir::Function, Body>,
    consts: &ArenaMap<flux_hir::Const, Body>,
    values: &mut crate::eval::ConstValues,
    discriminants: &mut crate::Discriminants,
) -> Vec<flux_hir::Diagnostic> {
    let items = &hir.items;
    let names = |id: ConstId| items.const_name(id);
    let mut evaluator = crate::eval::Evaluator::new(items, &typed.sigs, bodies, consts, &names);
    let mut checked = evaluate_discriminants(items, &mut evaluator, discriminants);
    let mut wanted: Vec<flux_ty::ConstInstance> = items
        .consts
        .iter()
        .filter(|&(id, data)| data.owner.is_none() && consts.get(id).is_some())
        .map(|(konst, _)| flux_ty::ConstInstance { konst, args: &[] })
        .collect();
    let concrete = bodies
        .iter()
        .filter(|&(id, _)| {
            let data = &items.functions[id];
            data.generics.is_empty() && flux_ty::traits::owner_param_count(items, data.owner) == 0
        })
        .map(|(_, body)| body)
        .chain(
            consts
                .iter()
                .filter(|&(id, _)| !is_generic_const(items, id))
                .map(|(_, body)| body),
        );
    for body in concrete {
        for c in body.const_items() {
            if !c.args.iter().any(|a| a.has_params()) && !wanted.contains(&c) {
                wanted.push(c);
            }
        }
    }
    for &c in &wanted {
        if let Ok(value) = evaluator.eval_instance(c) {
            if c.args.is_empty() {
                values.items.insert(c.konst, value);
            } else {
                values.instances.insert(c, value);
            }
        }
    }
    let mut diagnostics = eval_failures(items, &evaluator);
    diagnostics.append(&mut checked);
    for (id, _) in items.consts.iter() {
        if consts.get(id).is_some()
            && !is_generic_const(items, id)
            && values.items.get(id).is_none()
            && let Ok(value) = evaluator.eval_const(id)
        {
            values.items.insert(id, value);
        }
    }
    diagnostics
}

/// Evaluates the discriminants of every enum with some written as
/// expressions other than literals, into `table`, and checks them as
/// rustc does: one counted past the type's maximum is E0370 (and wraps
/// around), and a value assigned more than once E0081, at the enum.
/// Evaluation errors are the evaluator's failures.
fn evaluate_discriminants(
    items: &ItemTree,
    evaluator: &mut crate::eval::Evaluator<'_>,
    table: &mut crate::Discriminants,
) -> Vec<flux_hir::Diagnostic> {
    let mut diagnostics = Vec::new();
    for (id, s) in items.structs.iter() {
        if !s.has_const_discriminants() {
            continue;
        }
        let int = flux_ty::discriminant_ty(items, id);
        let error = |range, message: String| flux_hir::Diagnostic {
            file: s.loc.file,
            range,
            message,
        };
        let mut values: Vec<u128> = Vec::new();
        let mut failed = false;
        for (v, variant) in s.variants.iter().flatten().enumerate() {
            let value = match (variant.discriminant, values.last()) {
                (flux_hir::Discriminant::Const { offset, .. }, Some(&previous)) if offset > 0 => {
                    let next = crate::eval::truncate(previous.wrapping_add(1), int);
                    let wrapped = if int.is_signed() {
                        next == 1u128 << (int.bits() - 1)
                    } else {
                        next == 0
                    };
                    if wrapped {
                        diagnostics.push(error(
                            variant.name_range,
                            "enum discriminant overflowed".to_owned(),
                        ));
                    }
                    next
                }
                _ => match evaluator.discriminant(id, v as u32) {
                    Ok(bits) => bits,
                    Err(_) => {
                        failed = true;
                        break;
                    }
                },
            };
            values.push(value);
        }
        if failed {
            continue;
        }
        let mut reported = Vec::new();
        for (i, &value) in values.iter().enumerate() {
            if values[..i].contains(&value) && !reported.contains(&value) {
                reported.push(value);
                let shown = if int.is_signed() {
                    crate::eval::sign_extend(value, int).to_string()
                } else {
                    value.to_string()
                };
                diagnostics.push(error(
                    s.def_range,
                    format!("discriminant value `{shown}` assigned more than once"),
                ));
            }
        }
        table.insert(id, values.into());
    }
    diagnostics
}

/// Whether constant `id` has generic parameters: one of a trait, or of
/// a generic `impl` block.
fn is_generic_const(items: &flux_hir::ItemTree, id: ConstId) -> bool {
    match items.consts[id].owner {
        Some(flux_hir::Owner::Trait(_)) => true,
        Some(flux_hir::Owner::Impl(imp)) => !items.impls[imp].generics.is_empty(),
        None => false,
    }
}

/// Builds the MIR of constant `konst`'s value: a body without
/// parameters returning it.
pub fn build_const(hir: &Hir, typed: &TypedFile, konst: ConstId) -> Body {
    let (body, source_map) = hir.const_body(konst).expect("constants built have values");
    let sig = flux_ty::FnSig {
        params: Box::default(),
        ret: typed.sigs.consts[konst],
    };
    build_body(
        hir,
        typed,
        body,
        source_map,
        &typed.consts[konst],
        &sig,
        Item::Const(konst),
    )
}

/// Builds the MIR of `function`. Its body must have lowered and
/// type-checked without errors; anything erroneous becomes unreachable.
pub fn build(hir: &Hir, typed: &TypedFile, function: FunctionId) -> Body {
    let (body, source_map) = hir.body(function).expect("functions built have bodies");
    // Its associated types as its bounds and `impl` blocks say.
    let env = flux_ty::traits::projections_of(&hir.items, &typed.sigs, function);
    let norm = |ty| flux_ty::traits::normalize(&hir.items, &typed.sigs, &env, ty);
    let sig = &typed.sigs[function];
    let sig = &flux_ty::FnSig {
        params: sig.params.iter().map(|&p| norm(p)).collect(),
        // What its body returns: its opaque types' hidden types.
        ret: norm(flux_ty::traits::reveal_own(&typed.sigs, function, sig.ret)),
    };
    let data = &hir.items.functions[function];
    let mut built = build_body(
        hir,
        typed,
        body,
        source_map,
        &typed.bodies[function],
        sig,
        Item::Function(function),
    );
    built.inherits_overflow_checks = data.inherits_overflow_checks;
    built
}

/// What a body is of.
#[derive(Clone, Copy)]
enum Item {
    Function(FunctionId),
    Const(ConstId),
    /// The closure expression `expr` in `owner`'s body.
    Closure(FunctionId, ExprId),
}

/// Builds the MIR of closure `expr` of `owner`'s body, as the method of
/// its own kind's trait: `call(&self, args)`, `call_mut(&mut self,
/// args)` or `call_once(self, args)`, with its parameters bound from
/// the tuple `args` and what it captures read through `self`.
pub fn build_closure(hir: &Hir, typed: &TypedFile, owner: FunctionId, expr: ExprId) -> Body {
    let (body, source_map) = hir.body(owner).expect("closures are in bodies");
    let types = &typed.bodies[owner];
    let info = &types.closures[expr];
    let TyKind::Closure(_, args) = info.ty.kind() else {
        unreachable!("closures have closure types")
    };
    let parts = flux_ty::ClosureArgs::split(args);
    let env = flux_ty::traits::projections_of(&hir.items, &typed.sigs, owner);
    let norm = |ty| flux_ty::traits::normalize(&hir.items, &typed.sigs, &env, ty);
    let sig = flux_ty::FnSig {
        params: Box::new([closure_self(info.kind, info.ty), norm(parts.params())]),
        ret: norm(parts.ret()),
    };
    build_body(
        hir,
        typed,
        body,
        source_map,
        types,
        &sig,
        Item::Closure(owner, expr),
    )
}

/// A captured place as written: `x`, `s.a`, `*r`.
fn capture_name(
    body: &flux_hir::Body,
    binding_types: &ArenaMap<Binding, Ty>,
    capture: &flux_ty::Capture,
    sigs: &flux_ty::Signatures,
    items: &ItemTree,
) -> String {
    let mut text = body.bindings[capture.binding].name.to_string();
    let mut ty = binding_types
        .get(capture.binding)
        .copied()
        .unwrap_or(Ty::ERROR);
    let proj = &capture.projection;
    for (i, &p) in proj.iter().enumerate() {
        match p {
            flux_ty::CaptureProj::Deref => {
                if !matches!(proj.get(i + 1), Some(flux_ty::CaptureProj::Field(_))) {
                    text = format!("*{text}");
                }
                ty = match ty.kind() {
                    TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => t,
                    _ => Ty::ERROR,
                };
            }
            flux_ty::CaptureProj::Field(f) => {
                match ty.kind() {
                    TyKind::Adt(adt, _) => {
                        let name = items.structs[adt.strukt].variant(0).fields[f as usize].name;
                        text = format!("{text}.{name}");
                    }
                    _ => text = format!("{text}.{f}"),
                }
                ty = sigs
                    .fields(ty)
                    .and_then(|fields| fields.get(f as usize).copied())
                    .unwrap_or(Ty::ERROR);
            }
        }
    }
    text
}

/// The type of `self` in the method of trait `kind` of a closure of type
/// `ty`.
fn closure_self(kind: flux_ty::ClosureKind, ty: Ty) -> Ty {
    match kind {
        flux_ty::ClosureKind::Fn => Ty::new(TyKind::Ref(Mutability::Not, ty)),
        flux_ty::ClosureKind::FnMut => Ty::new(TyKind::Ref(Mutability::Mut, ty)),
        flux_ty::ClosureKind::FnOnce => ty,
    }
}

/// The bodies of closure `expr` of `owner` as the methods of the traits
/// it implements beyond its own kind's, each a call of that one: as
/// `call_mut` of an `Fn` closure, `call(&*self, args)`; as `call_once`,
/// `call(&self, args)` and then a drop of `self`.
pub fn closure_shims(
    hir: &Hir,
    typed: &TypedFile,
    owner: FunctionId,
    expr: ExprId,
) -> Vec<(flux_ty::ClosureKind, Body)> {
    use flux_ty::ClosureKind;
    let items = &hir.items;
    let types = &typed.bodies[owner];
    let info = &types.closures[expr];
    let TyKind::Closure(_, args) = info.ty.kind() else {
        return Vec::new();
    };
    let parts = flux_ty::ClosureArgs::split(args);
    let env = flux_ty::traits::projections_of(items, &typed.sigs, owner);
    let norm = |ty| flux_ty::traits::normalize(items, &typed.sigs, &env, ty);
    let (params, ret) = (norm(parts.params()), norm(parts.ret()));
    let range = hir.body(owner).map_or_else(TextRange::default, |(_, map)| {
        map.exprs.get(expr).copied().unwrap_or_default()
    });
    let method = |kind: ClosureKind| {
        let t = flux_ty::fn_trait(items, kind)?;
        let name = match kind {
            ClosureKind::Fn => "call",
            ClosureKind::FnMut => "call_mut",
            ClosureKind::FnOnce => "call_once",
        };
        items.traits[t].function(items, Symbol::intern(name))
    };
    let Some(target) = method(info.kind) else {
        return Vec::new();
    };
    let mut shims = Vec::new();
    for mode in [ClosureKind::FnMut, ClosureKind::FnOnce] {
        if mode <= info.kind {
            continue;
        }
        let mut locals = Arena::new();
        locals.alloc(LocalDecl {
            ty: ret,
            kind: LocalKind::Return,
        });
        let this = locals.alloc(LocalDecl {
            ty: closure_self(mode, info.ty),
            kind: LocalKind::Param,
        });
        let tuple = locals.alloc(LocalDecl {
            ty: params,
            kind: LocalKind::Param,
        });
        let borrow = locals.alloc(LocalDecl {
            ty: closure_self(info.kind, info.ty),
            kind: LocalKind::Temp,
        });
        let m = match info.kind {
            ClosureKind::Fn => Mutability::Not,
            _ => Mutability::Mut,
        };
        let place: Place = match mode {
            ClosureKind::FnOnce => this.into(),
            _ => Place::from(this).deref(),
        };
        let mut blocks = Arena::new();
        let after = BasicBlock::from_raw(1);
        let terminator = |kind| Terminator { kind, range };
        blocks.alloc(BasicBlockData {
            statements: vec![Statement {
                kind: StatementKind::Assign(
                    borrow.into(),
                    Rvalue::Ref {
                        mutability: m,
                        place,
                        two_phase: false,
                    },
                ),
                range,
            }],
            terminator: terminator(TerminatorKind::Call {
                func: target,
                generic_args: Ty::list(&[info.ty, params]),
                args: vec![Operand::Move(borrow.into()), Operand::Move(tuple.into())],
                dest: Body::RETURN.into(),
                target: Some(after),
            }),
        });
        // `self` is dropped once called.
        if mode == ClosureKind::FnOnce {
            blocks.alloc(BasicBlockData {
                statements: Vec::new(),
                terminator: terminator(TerminatorKind::Drop {
                    place: this.into(),
                    target: BasicBlock::from_raw(2),
                }),
            });
        }
        blocks.alloc(BasicBlockData {
            statements: Vec::new(),
            terminator: terminator(TerminatorKind::Return),
        });
        shims.push((
            mode,
            Body {
                locals,
                blocks,
                param_count: 2,
                c_strings: Vec::new(),
                in_core: false,
                inherits_overflow_checks: false,
                live_drops: Vec::new(),
                init_errors: Vec::new(),
                upvars: None,
                capture_uses: Vec::new(),
                owner: None,
                promoted: Vec::new(),
                bugs: Vec::new(),
                fn_spans: Default::default(),
            },
        ));
    }
    shims
}

fn build_body(
    hir: &Hir,
    typed: &TypedFile,
    body: &flux_hir::Body,
    source_map: &flux_hir::BodySourceMap,
    types: &InferenceResult,
    sig: &flux_ty::FnSig,
    item: Item,
) -> Body {
    let module = match item {
        Item::Function(f) | Item::Closure(f, _) => hir.items.functions[f].loc.module,
        Item::Const(c) => hir.items.consts[c].loc.module,
    };
    // The bounds in scope, for what is `Copy`.
    let env = match item {
        Item::Function(f) | Item::Closure(f, _) => flux_ty::traits::elaborate(
            &typed.sigs,
            &flux_ty::traits::predicates_of(&hir.items, &typed.sigs, f),
        ),
        Item::Const(_) => Vec::new(),
    };
    let projections = match item {
        Item::Function(f) | Item::Closure(f, _) => {
            flux_ty::traits::projections_of(&hir.items, &typed.sigs, f)
        }
        Item::Const(_) => Vec::new(),
    };
    let mut b = Builder {
        typed,
        items: &hir.items,
        body,
        source_map,
        types,
        locals: Arena::new(),
        blocks: Vec::new(),
        current: None,
        binding_locals: ArenaMap::new(),
        binding_ranges: ArenaMap::new(),
        guard_places: ArenaMap::new(),
        guard: None,
        errors: Vec::new(),
        loops: ArenaMap::new(),
        scopes: vec![Scope {
            kind: ScopeKind::Function,
            locals: Vec::new(),
        }],
        c_strings: Vec::new(),
        env,
        projections,
        extended_refs: Vec::new(),
        extend_at: None,
        extending: false,
        needs_drop: Default::default(),
        upvars: Vec::new(),
        capture_uses: Vec::new(),
        promoted: Vec::new(),
        storage_temps: Default::default(),
        bugs: Vec::new(),
        fn_spans: Default::default(),
    };
    b.locals.alloc(LocalDecl {
        ty: sig.ret,
        kind: LocalKind::Return,
    });
    let params: Vec<Local> = sig
        .params
        .iter()
        .map(|&ty| {
            b.locals.alloc(LocalDecl {
                ty,
                kind: LocalKind::Param,
            })
        })
        .collect();
    let entry = b.new_block();
    b.current = Some(entry);
    let (root, param_pats) = match item {
        Item::Closure(_, expr) => {
            let Expr::Closure {
                params: pats,
                body: closure_body,
                ..
            } = &body.exprs[expr]
            else {
                unreachable!("closures are closure expressions")
            };
            // What it captures, through `self`.
            let info = &types.closures[expr];
            let this = Place::from(params[0]);
            let env = match info.kind {
                flux_ty::ClosureKind::FnOnce => this,
                _ => this.deref(),
            };
            for (i, capture) in info.captures.iter().enumerate() {
                let field = env.field(i as u32);
                let place = match capture.mode {
                    flux_ty::CaptureMode::ByValue => field,
                    flux_ty::CaptureMode::ByRef(_) => field.deref(),
                };
                b.upvars.push((capture.clone(), place));
            }
            let at = b.range(expr);
            b.scopes[0].locals.push((params[0], at));
            b.scopes[0].locals.push((params[1], at));
            // The parameters, from the tuple of the arguments.
            for (i, &pat) in pats.iter().enumerate() {
                let range = b.pat_range(pat);
                b.declare_bindings(pat);
                let place = Place::from(params[1]).field(i as u32);
                b.match_irrefutable(pat, place.into(), range);
            }
            (*closure_body, &[][..])
        }
        _ => (body.root, &body.params[..]),
    };
    for (&pat, &local) in param_pats.iter().zip(&params) {
        let range = b.pat_range(pat);
        b.scopes[0].locals.push((local, range));
        match body.pats[pat] {
            Pat::Bind { binding, sub: None } if types.binding_refs.get(binding).is_none() => {
                let data = &body.bindings[binding];
                b.locals[local].kind = LocalKind::User {
                    name: data.name,
                    mutable: data.mutable,
                };
                b.binding_locals.insert(binding, local);
            }
            _ => {
                b.declare_bindings(pat);
                b.match_irrefutable(pat, Place::from(local).into(), range);
            }
        }
    }
    let upvars = match item {
        Item::Closure(_, expr) => {
            let info = &types.closures[expr];
            Some(crate::Upvars {
                kind: info.kind,
                names: info
                    .captures
                    .iter()
                    .map(|c| capture_name(body, &types.binding_types, c, &typed.sigs, &hir.items))
                    .collect(),
                by_ref: info
                    .captures
                    .iter()
                    .map(|c| matches!(c.mode, flux_ty::CaptureMode::ByRef(_)))
                    .collect(),
                mutable: info
                    .captures
                    .iter()
                    .map(|c| {
                        body.bindings[c.binding].mutable
                            && !c.projection.contains(&flux_ty::CaptureProj::Deref)
                    })
                    .collect(),
                annotated_refs: !info.hinted
                    && match &body.exprs[expr] {
                        Expr::Closure { param_types, .. } => param_types
                            .iter()
                            .flatten()
                            .any(|&t| has_ref(&body.types[t])),
                        _ => false,
                    },
            })
        }
        _ => None,
    };
    let range = b.range(root);
    // A constant's value keeps what it borrows, which is never dropped.
    if let Item::Const(_) = item {
        let mut extended = Vec::new();
        b.extending_refs(body.root, &mut extended);
        b.extended_refs = extended;
        b.extend_at = Some((usize::MAX, 0));
    }
    b.scoped(ScopeKind::Temps, range, |b| {
        b.lower_into(root, &Body::RETURN.into());
    });
    b.leave_scopes(0, false, range);
    b.terminate(TerminatorKind::Return, range);

    let blocks = b
        .blocks
        .into_iter()
        .map(|(statements, terminator)| BasicBlockData {
            statements,
            terminator: terminator.unwrap_or(Terminator {
                kind: TerminatorKind::Unreachable,
                range: TextRange::default(),
            }),
        });
    let mut arena = Arena::new();
    for block in blocks {
        arena.alloc(block);
    }
    // A type that is an error was reported where it was made, and
    // programs with errors are not compiled: one that reaches MIR
    // without a diagnostic would compile to nonsense.
    if b.locals.values().any(|l| l.ty.has_error()) {
        let at = (types.expr_types.iter())
            .filter(|(_, ty)| ty.has_error())
            .filter_map(|(e, _)| source_map.exprs.get(e).copied())
            .min_by_key(|r| (r.start(), std::cmp::Reverse(r.end())));
        b.bugs.push((
            at.unwrap_or(range),
            "a type that is an error, without a diagnostic",
        ));
    }
    let mut built = Body {
        locals: b.locals,
        blocks: arena,
        param_count: params.len(),
        c_strings: b.c_strings,
        in_core: hir.items.is_core(module),
        inherits_overflow_checks: false,
        live_drops: Vec::new(),
        init_errors: Vec::new(),
        upvars,
        capture_uses: b.capture_uses,
        owner: match item {
            Item::Function(f) | Item::Closure(f, _) => Some(f),
            Item::Const(_) => None,
        },
        promoted: b.promoted,
        bugs: b.bugs,
        fn_spans: b.fn_spans,
    };
    built.init_errors = crate::init::check(&built, &hir.items, &typed.sigs);
    built.init_errors.extend(b.errors);
    let precise = match item {
        Item::Function(f) => hir.items.functions[f].precise_live_drops,
        Item::Const(_) | Item::Closure(..) => false,
    };
    crate::drops::elaborate(&mut built, &hir.items, &typed.sigs, &b.env, precise);
    built
}

/// What a guard may not change: as rustc's guards borrow the place matched
/// and see `ref mut` bindings through shared references.
struct Guard {
    scrutinee: Option<Place>,
    ref_muts: Vec<BindingId>,
}

/// A place a pattern is matched against. In a closure it may be one the
/// closure captures only parts of: `|| match r { S { v } => .. }`
/// captures `(*r).v`, not `r`. Such a place is resolved to what the
/// closure captures as the pattern goes into it, where it is read.
#[derive(Clone)]
enum PatPlace {
    Place(Place),
    Upvar(BindingId, Vec<flux_ty::CaptureProj>),
}

impl PatPlace {
    fn deref(&self) -> PatPlace {
        match self {
            PatPlace::Place(place) => PatPlace::Place(place.deref()),
            PatPlace::Upvar(binding, proj) => {
                let mut proj = proj.clone();
                proj.push(flux_ty::CaptureProj::Deref);
                PatPlace::Upvar(*binding, proj)
            }
        }
    }

    fn field(&self, index: u32) -> PatPlace {
        match self {
            PatPlace::Place(place) => PatPlace::Place(place.field(index)),
            PatPlace::Upvar(binding, proj) => {
                let mut proj = proj.clone();
                proj.push(flux_ty::CaptureProj::Field(index));
                PatPlace::Upvar(*binding, proj)
            }
        }
    }
}

impl From<Place> for PatPlace {
    fn from(place: Place) -> Self {
        PatPlace::Place(place)
    }
}

/// Where control goes once a pattern matched: the block, the place each
/// binding binds, and where to go to try the next way of matching (the
/// next alternative of an or-pattern) if a guard fails.
struct Exit {
    block: BasicBlock,
    binds: Vec<(BindingId, Place)>,
    resume: BasicBlock,
}

/// The patterns of a tuple or tuple struct pattern with the indices of
/// the fields they match, of `count`, around `..` at `rest`.
fn positions(
    pats: &[PatId],
    rest: Option<u32>,
    count: usize,
) -> impl Iterator<Item = (u32, PatId)> + '_ {
    pats.iter().enumerate().map(move |(i, &p)| {
        let index = match rest {
            Some(rest) if i >= rest as usize => count - (pats.len() - i),
            _ => i,
        };
        (index as u32, p)
    })
}

/// A scope whose locals are dropped, in reverse order, when it ends.
struct Scope {
    kind: ScopeKind,
    /// Each with where it was made: its binding, or its expression.
    locals: Vec<(Local, TextRange)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    /// The parameters, which drop after everything else.
    Function,
    /// A block's bindings, which also die (`StorageDead`), and the
    /// temporaries extended to live as long as them.
    Block,
    /// The temporaries of a statement, of a block's tail (dropped before
    /// its bindings, as in edition 2024), or of a condition.
    Temps,
}

#[derive(Clone)]
struct LoopScope {
    break_block: BasicBlock,
    continue_block: BasicBlock,
    /// Where `break value` stores the value.
    dest: Place,
    /// How many scopes were open outside the loop; leaving it ends the
    /// others.
    scope_depth: usize,
}

struct Builder<'a> {
    typed: &'a TypedFile,
    items: &'a ItemTree,
    body: &'a flux_hir::Body,
    source_map: &'a flux_hir::BodySourceMap,
    types: &'a InferenceResult,
    locals: Arena<LocalDecl>,
    /// Blocks under construction; a block is finished once it has a
    /// terminator.
    blocks: Vec<(Vec<Statement>, Option<Terminator>)>,
    /// The block being appended to; `None` right after a terminator, until
    /// code needs a block again (then it starts an unreachable one).
    current: Option<BasicBlock>,
    binding_locals: ArenaMap<Binding, Local>,
    /// Each binding's pattern, where it is reported.
    binding_ranges: ArenaMap<Binding, TextRange>,
    /// While lowering a guard, the bindings of its arm that bind values
    /// that are not `Copy`: the places they will bind, which the guard
    /// reads instead, as rustc's guards see them by reference.
    guard_places: ArenaMap<Binding, Place>,
    /// While lowering a guard, what it may not change.
    guard: Option<Guard>,
    /// Errors found while building: what a guard may not do.
    errors: Vec<flux_hir::Diagnostic>,
    loops: ArenaMap<Expr, LoopScope>,
    /// The enclosing scopes, innermost last.
    scopes: Vec<Scope>,
    c_strings: Vec<Box<[u8]>>,
    /// The bounds in scope, elaborated: what is `Copy`, and what needs
    /// dropping.
    env: Vec<flux_ty::Predicate>,
    /// What the bounds in scope say associated types are: the types of
    /// fields like `T::Out`.
    projections: Vec<flux_ty::traits::Projection>,
    /// In a `let` initializer, the borrows whose temporaries live as long
    /// as the `let`'s bindings: `&make()` in `let r = &make();`.
    extended_refs: Vec<ExprId>,
    /// Where those temporaries go: a block scope, and the position of the
    /// `let`'s bindings in it, before which they are dropped.
    extend_at: Option<(usize, usize)>,
    /// Lowering the place of one of those borrows: its temporary is
    /// extended.
    extending: bool,
    /// Whether each type asked about needs dropping, which may take the
    /// trait solver.
    needs_drop: std::cell::RefCell<rustc_hash::FxHashMap<Ty, bool>>,
    /// In a closure's body, what it captures and where each is, through
    /// `self`: the place itself if captured by value, else what the
    /// reference to it points to.
    upvars: Vec<(flux_ty::Capture, Place)>,
    /// [`Body::capture_uses`].
    capture_uses: Vec<(BasicBlock, usize, TextRange, bool)>,
    /// The temporaries that may be borrowed, which die where their
    /// scope ends.
    storage_temps: rustc_hash::FxHashSet<Local>,
    /// [`Body::promoted`].
    promoted: Vec<crate::eval::Value>,
    /// [`Body::bugs`].
    bugs: Vec<(TextRange, &'static str)>,
    /// [`Body::fn_spans`].
    fn_spans: rustc_hash::FxHashMap<BasicBlock, u32>,
}

/// Whether `expr` denotes a place: a local, a field or element of one, or
/// what a pointer points to.
/// Whether type `ty`, as written, has a reference.
fn has_ref(ty: &flux_hir::TypeRef) -> bool {
    use flux_hir::TypeRef as T;
    match ty {
        T::Ref { .. } => true,
        T::Tuple(list) => list.iter().any(has_ref),
        T::Ptr { pointee, .. } | T::Array { elem: pointee, .. } | T::Slice(pointee) => {
            has_ref(pointee)
        }
        T::Path(path) => path
            .args
            .iter()
            .flatten()
            .any(|a| !a.lifetimes.is_empty() || a.types.iter().any(has_ref)),
        // A trait object's lifetime bound.
        T::Dyn(_) => true,
        T::FnPtr { .. }
        | T::Never
        | T::Infer
        | T::Const(_)
        | T::ImplTrait(_)
        | T::ImplParam(_)
        | T::Opaque(_)
        | T::Error => false,
    }
}

fn is_place(body: &flux_hir::Body, expr: ExprId) -> bool {
    match body.exprs[expr] {
        Expr::Path(Res::Local(_) | Res::Static(_)) => true,
        Expr::Field { expr, .. } | Expr::Index { base: expr, .. } => is_place(body, expr),
        Expr::Unary {
            op: UnaryOp::Deref, ..
        } => true,
        _ => false,
    }
}

impl Builder<'_> {
    fn range(&self, expr: ExprId) -> TextRange {
        self.source_map.exprs.get(expr).copied().unwrap_or_default()
    }

    fn pat_range(&self, pat: PatId) -> TextRange {
        self.source_map.pats.get(pat).copied().unwrap_or_default()
    }

    /// The type of `expr`'s value where it is used: a function item that
    /// coerces to a pointer is the pointer.
    fn expr_ty(&self, expr: ExprId) -> Ty {
        let ty = self.own_ty(expr);
        match ty.kind() {
            TyKind::FnDef(def) if self.types.reified.get(expr).is_some() => {
                Ty::new(TyKind::FnPtr(def.sig))
            }
            TyKind::Closure(_, args) if self.types.reified.get(expr).is_some() => {
                let parts = flux_ty::ClosureArgs::split(args);
                let params = match parts.params().kind() {
                    TyKind::Tuple(list) => list,
                    _ => &[],
                };
                Ty::new(TyKind::FnPtr(flux_ty::FnSigTy::new(
                    params,
                    parts.ret(),
                    false,
                )))
            }
            _ => ty,
        }
    }

    /// The type of `expr` itself, before coercions.
    fn own_ty(&self, expr: ExprId) -> Ty {
        self.types
            .expr_types
            .get(expr)
            .copied()
            .unwrap_or(Ty::ERROR)
    }

    /// The type of the value `expr` gives its context: an array reference
    /// unsized to a slice reference is one, a `&mut T` reborrowed as a
    /// `&T` is that, and a `&&T` deref-coerced to a `&T` is that.
    fn value_ty(&self, expr: ExprId) -> Ty {
        if let Some(&ty) = self.types.unsizes.get(expr) {
            return ty;
        }
        let ty = self.expr_ty(expr);
        let Some(&m) = self.types.reborrows.get(expr) else {
            return ty;
        };
        let derefs = self.types.deref_coercions.get(expr).copied().unwrap_or(1);
        let mut pointee = ty;
        for _ in 0..derefs {
            match pointee.kind() {
                TyKind::Ref(_, inner) => pointee = inner,
                _ => return ty,
            }
        }
        Ty::new(TyKind::Ref(m, pointee))
    }

    fn new_block(&mut self) -> BasicBlock {
        self.blocks.push((Vec::new(), None));
        BasicBlock::from_raw(self.blocks.len() as u32 - 1)
    }

    /// The block to append to, starting a fresh (unreachable) one after a
    /// terminator.
    fn block(&mut self) -> BasicBlock {
        match self.current {
            Some(block) => block,
            None => {
                let block = self.new_block();
                self.current = Some(block);
                block
            }
        }
    }

    fn temp(&mut self, ty: Ty) -> Local {
        self.locals.alloc(LocalDecl {
            ty,
            kind: LocalKind::Temp,
        })
    }

    /// A temporary holding `expr`'s value, evaluated now, and dropped
    /// where the innermost scope ends if its type needs it: after the
    /// temporaries made while evaluating it, as rustc schedules drops
    /// once a value exists.
    fn temp_for(&mut self, expr: ExprId) -> Local {
        let ty = self.value_ty(expr);
        let temp = self.temp(ty);
        self.lower_into(expr, &temp.into());
        if self.needs_drop(ty) {
            let range = self.range(expr);
            let scope = self.scopes.last_mut().expect("bodies have a scope");
            scope.locals.push((temp, range));
        }
        temp
    }

    fn needs_drop(&self, ty: Ty) -> bool {
        if let Some(&known) = self.needs_drop.borrow().get(&ty) {
            return known;
        }
        let needs = flux_ty::traits::needs_drop(self.items, &self.typed.sigs, &self.env, ty);
        self.needs_drop.borrow_mut().insert(ty, needs);
        needs
    }

    /// Runs `f` in a new scope of `kind`, then ends it.
    fn scoped<R>(
        &mut self,
        kind: ScopeKind,
        range: TextRange,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.scopes.push(Scope {
            kind,
            locals: Vec::new(),
        });
        let result = f(self);
        let depth = self.scopes.len() - 1;
        self.leave_scopes(depth, true, range);
        self.scopes.pop();
        result
    }

    /// Drops `place` here.
    fn drop_place(&mut self, place: Place, range: TextRange) {
        if self.current.is_none() {
            return;
        }
        let target = self.new_block();
        self.terminate(TerminatorKind::Drop { place, target }, range);
        self.current = Some(target);
    }

    fn assign(&mut self, dest: &Place, rvalue: Rvalue, range: TextRange) {
        let block = self.block();
        self.blocks[block.index()].0.push(Statement {
            kind: StatementKind::Assign(dest.clone(), rvalue),
            range,
        });
    }

    /// Stores `()` in `dest`, unless the code is unreachable or `dest` is
    /// of type `!`: the end of a block that diverged.
    fn assign_unit(&mut self, dest: &Place, range: TextRange) {
        if self.current.is_none() || self.place_ty(dest) == Ty::NEVER {
            return;
        }
        self.assign(dest, Rvalue::Use(Operand::Const(Constant::Unit)), range);
    }

    /// Reads `place`: copies a `Copy` value, moves any other.
    fn read(&self, place: Place) -> Operand {
        let ty = self.place_ty(&place);
        if flux_ty::traits::is_copy(self.items, &self.typed.sigs, &self.env, ty) {
            Operand::Copy(place)
        } else {
            Operand::Move(place)
        }
    }

    fn place_ty(&self, place: &Place) -> Ty {
        let local_ty = PlaceTy::from(self.locals[place.local].ty);
        let ty = place.projection.iter().fold(local_ty, |ty, &elem| {
            let ty = ty.project(elem, &self.typed.sigs);
            if ty.ty.has_aliases() {
                let normalized = flux_ty::traits::normalize(
                    self.items,
                    &self.typed.sigs,
                    &self.projections,
                    ty.ty,
                );
                PlaceTy {
                    ty: normalized,
                    ..ty
                }
            } else {
                ty
            }
        });
        ty.ty
    }

    /// Leaves the scopes from `depth` in, if the code is reachable:
    /// drops their locals, innermost first, and with `kill`, ends their
    /// bindings' storage, at `range`.
    fn leave_scopes(&mut self, depth: usize, kill: bool, range: TextRange) {
        let locals: Vec<(Local, TextRange, bool)> = self.scopes[depth..]
            .iter()
            .rev()
            .flat_map(|scope| {
                let block = scope.kind == ScopeKind::Block;
                scope.locals.iter().rev().map(move |&(l, r)| (l, r, block))
            })
            .collect();
        for (local, at, block) in locals {
            if self.current.is_none() {
                return;
            }
            if self.needs_drop(self.locals[local].ty) {
                self.drop_place(local.into(), at);
            }
            let user = matches!(self.locals[local].kind, LocalKind::User { .. });
            if kill && block && user {
                let block = self.block();
                self.blocks[block.index()].0.push(Statement {
                    kind: StatementKind::StorageDead(local),
                    range,
                });
            } else if kill && self.storage_temps.contains(&local) {
                // A temporary's, where it was made, as rustc reports
                // one dropped while borrowed.
                let block = self.block();
                self.blocks[block.index()].0.push(Statement {
                    kind: StatementKind::StorageDead(local),
                    range: at,
                });
            }
        }
    }

    fn terminate(&mut self, kind: TerminatorKind, range: TextRange) {
        let block = self.block();
        self.blocks[block.index()].1 = Some(Terminator { kind, range });
        self.current = None;
    }

    fn goto(&mut self, target: BasicBlock, range: TextRange) {
        if self.current.is_some() {
            self.terminate(TerminatorKind::Goto(target), range);
        }
    }

    /// Evaluates `expr` to an operand: constants directly, anything else
    /// into a new temporary.
    fn lower_operand(&mut self, expr: ExprId) -> Operand {
        // A constant reference to an array coerced to one to a slice
        // (`f(b"ab")`) is unsized first.
        if self.types.unsizes.get(expr).is_some() {
            let temp = self.temp_for(expr);
            return self.read(temp.into());
        }
        self.direct_operand(expr)
    }

    /// `expr`'s value as an operand, without its coercions: a constant
    /// itself, anything else read from a temporary.
    fn direct_operand(&mut self, expr: ExprId) -> Operand {
        match &self.body.exprs[expr] {
            Expr::Literal(literal) => Operand::Const(self.constant(literal, expr)),
            Expr::Tuple(fields) if fields.is_empty() => Operand::Const(Constant::Unit),
            // A negated literal is one constant, as in rustc: `-128i8`
            // does not negate 128.
            Expr::Unary {
                op: UnaryOp::Neg,
                operand,
            } if matches!(
                self.body.exprs[*operand],
                Expr::Literal(Literal::Int { .. })
            ) =>
            {
                let (Expr::Literal(Literal::Int { value, .. }), Some(ty)) =
                    (&self.body.exprs[*operand], self.expr_ty(expr).as_int())
                else {
                    unreachable!("checked above")
                };
                Operand::Const(Constant::int((*value as i128).wrapping_neg(), ty))
            }
            Expr::Path(Res::ConstParam(_))
                if let Some(&param) = self.types.const_params.get(expr) =>
            {
                Operand::Const(Constant::of_ty_const(param, self.expr_ty(expr)))
            }
            Expr::Path(res @ (Res::Const(_) | Res::Assoc { .. }))
                if self.types.reborrows.get(expr).is_none()
                    && let Some(c) = self.const_of(expr, *res) =>
            {
                Operand::Const(Constant::Item {
                    id: c.konst,
                    args: c.args,
                    ty: self.expr_ty(expr),
                })
            }
            _ => {
                let temp = self.temp_for(expr);
                self.read(temp.into())
            }
        }
    }

    /// The place `expr` denotes: itself if it is a place expression,
    /// otherwise a temporary holding its value.
    fn lower_place(&mut self, expr: ExprId) -> Place {
        if let Some(place) = self.upvar_place(expr) {
            return place;
        }
        match self.body.exprs[expr] {
            Expr::Path(Res::Local(binding)) if self.guard_places.get(binding).is_some() => {
                self.guard_places[binding].clone()
            }
            Expr::Path(Res::Local(binding)) if self.binding_locals.get(binding).is_some() => {
                self.binding_locals[binding].into()
            }
            Expr::Field { expr: base, .. } => {
                let derefs = self.types.field_derefs.get(expr).copied().unwrap_or(0);
                self.lower_place_deref(base, derefs)
                    .field(self.field_index(expr))
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                operand,
            } => self.lower_place_deref(operand, 1),
            Expr::Index { base, index } if let Some(&instance) = self.types.overloads.get(expr) => {
                self.overloaded_index(expr, (base, index), instance)
            }
            Expr::Index { base, index } => {
                let derefs = self.types.field_derefs.get(expr).copied().unwrap_or(0);
                let place = self.lower_place_deref(base, derefs);
                self.index(place, index, self.range(expr))
            }
            Expr::Path(Res::Static(id)) => self.static_place(id, self.range(expr)),
            // Neither declared here nor captured: evaluating it for a
            // temporary would come back here.
            Expr::Path(Res::Local(_)) => {
                let range = self.range(expr);
                self.bug("a local neither declared nor captured", range);
                self.temp(self.value_ty(expr)).into()
            }
            _ => {
                // A temporary that may be borrowed dies where its scope
                // ends (`StorageDead`), whether it needs dropping or not.
                let temp = match (std::mem::take(&mut self.extending), self.extend_at) {
                    (true, Some((scope, at))) => {
                        let ty = self.value_ty(expr);
                        let temp = self.temp(ty);
                        self.lower_into(expr, &temp.into());
                        if scope != usize::MAX {
                            let range = self.range(expr);
                            let at = at.min(self.scopes[scope].locals.len());
                            self.scopes[scope].locals.insert(at, (temp, range));
                            self.extend_at = Some((scope, at + 1));
                        }
                        temp
                    }
                    _ => {
                        let temp = self.temp_for(expr);
                        if !self.needs_drop(self.locals[temp].ty) {
                            let range = self.range(expr);
                            let scope = self.scopes.last_mut().expect("bodies have a scope");
                            scope.locals.push((temp, range));
                        }
                        temp
                    }
                };
                self.storage_temps.insert(temp);
                temp.into()
            }
        }
    }

    /// `base[index]` through `Index::index(&base, index)` or
    /// `IndexMut::index_mut(&mut base, index)`, as type checking chose:
    /// the place the reference it returns points to. As in rustc, the
    /// base is borrowed before the index is evaluated, and not two-phase
    /// (`&mut a[..a.len()]` is E0502); it is unsized if it is an array
    /// indexed as a slice.
    fn overloaded_index(
        &mut self,
        expr: ExprId,
        (base, index): (ExprId, ExprId),
        instance: flux_ty::Instance,
    ) -> Place {
        let range = self.range(expr);
        let derefs = self.types.field_derefs.get(expr).copied().unwrap_or(0);
        let place = self.lower_place_deref(base, derefs);
        let mutability = if self.items.functions[instance.function].name.as_str() == "index_mut" {
            Mutability::Mut
        } else {
            Mutability::Not
        };
        let ty = self.place_ty(&place);
        let reference = self.temp(Ty::new(TyKind::Ref(mutability, ty)));
        let borrow = Rvalue::Ref {
            mutability,
            place,
            two_phase: false,
        };
        self.assign(&reference.into(), borrow, range);
        let mut receiver = reference;
        if self.types.unsized_indices.get(expr).is_some()
            && let TyKind::Array(elem, _) = ty.kind()
        {
            let slice = Ty::new(TyKind::Ref(mutability, Ty::slice(elem)));
            let unsized_ = self.temp(slice);
            let operand = self.read(reference.into());
            self.assign(&unsized_.into(), Rvalue::Unsize(operand, slice), range);
            receiver = unsized_;
        }
        let index = self.lower_operand(index);
        let output = Ty::new(TyKind::Ref(mutability, self.expr_ty(expr)));
        let result = self.temp(output);
        let args = vec![Operand::Move(receiver.into()), index];
        if let (Some(block), Some(bracket)) = (self.current, self.source_map.ops.get(expr)) {
            self.fn_spans.insert(block, bracket.start());
        }
        self.call(
            instance.function,
            instance.args,
            args,
            &result.into(),
            range,
        );
        Place::from(result).deref()
    }

    /// The element of the array or slice at `place` at `index`, after
    /// the check that `index` is within it, at `range`.
    fn index(&mut self, place: Place, index: ExprId, range: TextRange) -> Place {
        let usize = Ty::int(IntTy::Usize);
        let local = self.temp(usize);
        self.lower_into(index, &local.into());
        let len = match self.place_ty(&place).kind() {
            TyKind::Array(_, len) => Operand::Const(Constant::of_ty_const(len, usize)),
            _ => {
                let len = self.temp(usize);
                let rvalue = Rvalue::Len {
                    place: place.clone(),
                    borrows: false,
                };
                self.assign(&len.into(), rvalue, range);
                Operand::Copy(len.into())
            }
        };
        let within = self.temp(Ty::BOOL);
        let index = Operand::Copy(local.into());
        self.assign(
            &within.into(),
            Rvalue::BinaryOp(BinOp::Lt, index.clone(), len.clone()),
            range,
        );
        let msg = AssertMessage::BoundsCheck { len, index };
        self.assert(Operand::Copy(within.into()), true, msg, range);
        place.index(local)
    }

    /// The place `expr` denotes, dereferenced `derefs` times, as an
    /// autoderef (`r.get()`, `r.v`, `r[i]`) or a reborrow (`&*r`) does.
    /// In a closure, that place may be in what the closure captures when
    /// `expr`'s own place is not: `|| r.get()` captures `*r`, not `r`.
    fn lower_place_deref(&mut self, expr: ExprId, derefs: u32) -> Place {
        if derefs > 0
            && !self.upvars.is_empty()
            && let Some((binding, mut proj)) = self.hir_place(expr)
            && self.binding_locals.get(binding).is_none()
        {
            proj.extend((0..derefs).map(|_| flux_ty::CaptureProj::Deref));
            if let Some(place) = self.capture_source(binding, &proj) {
                return place;
            }
        }
        let mut place = self.lower_place(expr);
        for _ in 0..derefs {
            place = place.deref();
        }
        place
    }

    /// The place `expr` denotes, for a pattern to match: in a closure,
    /// perhaps one that is not captured itself but whose parts are.
    fn pat_place(&mut self, expr: ExprId) -> PatPlace {
        if let Some((binding, proj)) = self.hir_place(expr)
            && self.binding_locals.get(binding).is_none()
            && self.guard_places.get(binding).is_none()
            && self.capture_source(binding, &proj).is_none()
        {
            return PatPlace::Upvar(binding, proj);
        }
        PatPlace::Place(self.lower_place(expr))
    }

    /// `place` as a place here, which a pattern reads or binds.
    fn resolve(&mut self, place: PatPlace, range: TextRange) -> Place {
        match place {
            PatPlace::Place(place) => place,
            PatPlace::Upvar(binding, proj) => match self.capture_source(binding, &proj) {
                Some(place) => place,
                None => {
                    let ty = self.pat_place_ty(&PatPlace::Upvar(binding, proj));
                    self.bug(
                        "a pattern reads a place its closure does not capture",
                        range,
                    );
                    self.temp(ty).into()
                }
            },
        }
    }

    fn pat_place_ty(&self, place: &PatPlace) -> Ty {
        match place {
            PatPlace::Place(place) => self.place_ty(place),
            PatPlace::Upvar(binding, proj) => {
                let ty = self.types.binding_types.get(*binding).copied();
                let ty = PlaceTy::from(ty.unwrap_or(Ty::ERROR));
                let ty = proj.iter().fold(ty, |ty, &p| {
                    let elem = match p {
                        flux_ty::CaptureProj::Deref => ProjectionElem::Deref,
                        flux_ty::CaptureProj::Field(i) => ProjectionElem::Field(i),
                    };
                    let ty = ty.project(elem, &self.typed.sigs);
                    PlaceTy {
                        ty: self.normalize(ty.ty),
                        ..ty
                    }
                });
                ty.ty
            }
        }
    }

    /// Code that type checking rules out, which MIR building lowers to
    /// `unreachable` and records in [`Body::bugs`]: expected where the
    /// program has errors (bodies are built to check them, or to
    /// evaluate constants, before all errors are known), and a bug in
    /// flux anywhere else, which [`build_file`] reports.
    fn bug(&mut self, what: &'static str, range: TextRange) {
        self.bugs.push((range, what));
        self.terminate(TerminatorKind::Unreachable, range);
    }

    /// Static `id`, as a place: behind a temporary holding its address,
    /// `&T`, or `*mut T` for a `static mut`, as in rustc.
    fn static_place(&mut self, id: flux_hir::ConstId, range: TextRange) -> Place {
        let ty = self.typed.sigs.consts.get(id).copied().unwrap_or(Ty::ERROR);
        let pointer = if self.items.consts[id].is_static_mut() {
            Ty::raw_ptr(Mutability::Mut, ty)
        } else {
            Ty::new(TyKind::Ref(Mutability::Not, ty))
        };
        let temp = self.locals.alloc(LocalDecl {
            ty: pointer,
            kind: LocalKind::Static(id),
        });
        let address = Operand::Const(Constant::Static { id, ty: pointer });
        self.assign(&temp.into(), Rvalue::Use(address), range);
        Place::from(temp).deref()
    }

    /// In a closure's body, the place `expr` denotes if it is in a place
    /// the closure captures.
    fn upvar_place(&self, expr: ExprId) -> Option<Place> {
        if self.upvars.is_empty() {
            return None;
        }
        let (binding, proj) = self.hir_place(expr)?;
        if self.binding_locals.get(binding).is_some() {
            return None;
        }
        self.capture_source(binding, &proj)
    }

    /// The place of binding `binding`, then `proj`, here: in a local,
    /// or in what a closure captures.
    fn capture_source(&self, binding: BindingId, proj: &[flux_ty::CaptureProj]) -> Option<Place> {
        let (mut place, rest) = match self.binding_locals.get(binding) {
            Some(&local) => (Place::from(local), proj),
            None => {
                let (capture, place) = self
                    .upvars
                    .iter()
                    .find(|(c, _)| c.binding == binding && proj.starts_with(&c.projection))?;
                (place.clone(), &proj[capture.projection.len()..])
            }
        };
        for &p in rest {
            place = match p {
                flux_ty::CaptureProj::Deref => place.deref(),
                flux_ty::CaptureProj::Field(i) => place.field(i),
            };
        }
        Some(place)
    }

    /// The binding `expr` is a place in, and the path to it, if it is a
    /// place expression.
    fn hir_place(&self, expr: ExprId) -> Option<(BindingId, Vec<flux_ty::CaptureProj>)> {
        match self.body.exprs[expr] {
            Expr::Path(Res::Local(binding)) => Some((binding, Vec::new())),
            Expr::Field { expr: base, .. } => {
                let (binding, mut proj) = self.hir_place(base)?;
                let derefs = self.types.field_derefs.get(expr).copied().unwrap_or(0);
                proj.extend((0..derefs).map(|_| flux_ty::CaptureProj::Deref));
                proj.push(flux_ty::CaptureProj::Field(self.field_index(expr)));
                Some((binding, proj))
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                operand,
            } => {
                let (binding, mut proj) = self.hir_place(operand)?;
                proj.push(flux_ty::CaptureProj::Deref);
                Some((binding, proj))
            }
            _ => None,
        }
    }

    /// `|..| body`: the closure, from what it captures, each read or
    /// borrowed here.
    fn lower_closure(&mut self, expr: ExprId, dest: &Place, range: TextRange) {
        let Some(info) = self.types.closures.get(expr) else {
            self.bug("a closure without capture analysis", range);
            return;
        };
        let mut operands = Vec::new();
        for capture in info.captures.iter() {
            let Some(place) = self.capture_source(capture.binding, &capture.projection) else {
                self.bug("a capture of a place not found here", range);
                return;
            };
            let operand = match capture.mode {
                flux_ty::CaptureMode::ByValue => self.read(place),
                flux_ty::CaptureMode::ByRef(mutability) => {
                    let temp = self.temp(capture.upvar_ty());
                    let rvalue = Rvalue::Ref {
                        mutability,
                        place,
                        two_phase: false,
                    };
                    let block = self.block();
                    let index = self.blocks[block.index()].0.len();
                    self.capture_uses
                        .push((block, index, capture.range, capture.assigned));
                    self.assign(&temp.into(), rvalue, range);
                    Operand::Move(temp.into())
                }
            };
            operands.push(operand);
        }
        self.assign(
            dest,
            Rvalue::Aggregate(AggregateKind::Closure, operands),
            range,
        );
    }

    /// A method's receiver, or the value a call of a value calls,
    /// dereferenced and borrowed or moved as type checking said, in a
    /// temporary, so that errors about it point at it.
    fn adjusted_receiver(&mut self, receiver: ExprId, method: flux_ty::Method) -> Operand {
        let receiver_range = self.range(receiver);
        let place = self.lower_place_deref(receiver, method.derefs);
        let ty = match method.autoref {
            Some(m) => Ty::new(TyKind::Ref(m, self.place_ty(&place))),
            None => self.place_ty(&place),
        };
        let temp = self.temp(ty);
        // A receiver taken by value is moved in now, so it is
        // dropped if an argument leaves first.
        if method.autoref.is_none() && self.needs_drop(ty) {
            let scope = self.scopes.last_mut().expect("bodies have a scope");
            scope.locals.push((temp, receiver_range));
        }
        // rustc's two-phase borrows are for autoref alone, not unsizing:
        // `arr.swap(0, arr.len() - 1)` is E0502.
        let rvalue = match method.autoref {
            Some(mutability) => Rvalue::Ref {
                mutability,
                place,
                two_phase: mutability == Mutability::Mut && !method.unsize,
            },
            None => Rvalue::Use(self.read(place)),
        };
        self.assign(&temp.into(), rvalue, receiver_range);
        // `arr.iter()`: the reference to the array, as one to a slice.
        if method.unsize
            && let TyKind::Ref(m, pointee) = ty.kind()
            && let TyKind::Array(elem, _) = pointee.kind()
        {
            let slice = Ty::new(TyKind::Ref(m, Ty::new(TyKind::Slice(elem))));
            let unsized_ = self.temp(slice);
            let operand = self.read(temp.into());
            self.assign(
                &unsized_.into(),
                Rvalue::Unsize(operand, slice),
                receiver_range,
            );
            return self.read(unsized_.into());
        }
        self.read(temp.into())
    }

    /// `ty` with its associated types normalized by the bounds in scope.
    fn normalize(&self, ty: Ty) -> Ty {
        flux_ty::traits::normalize(self.items, &self.typed.sigs, &self.projections, ty)
    }

    fn field_index(&self, expr: ExprId) -> u32 {
        self.types.field_indices.get(expr).copied().unwrap_or(0)
    }

    /// The index of field `name` of `strukt`, or of its variant `variant`.
    fn struct_field(&self, strukt: StructId, variant: u32, name: Symbol) -> u32 {
        self.items.structs[strukt]
            .variant(variant)
            .field(name)
            .map_or(0, |(i, _)| i as u32)
    }

    /// The aggregate that builds `strukt`, or its variant `variant`.
    fn aggregate(&self, strukt: StructId, variant: u32) -> AggregateKind {
        if self.items.structs[strukt].is_enum() {
            AggregateKind::Variant(strukt, variant)
        } else {
            AggregateKind::Struct(strukt)
        }
    }

    fn constant(&mut self, literal: &Literal, expr: ExprId) -> Constant {
        match (literal, self.expr_ty(expr).as_int()) {
            (Literal::Int { value, .. }, Some(ty)) => Constant::int(*value as i128, ty),
            (Literal::Bool(value), _) => Constant::Bool(*value),
            (Literal::Char(c), _) => Constant::Char(*c),
            // A string's text and a byte string's bytes are promoted, as
            // rustc keeps them in static memory.
            (Literal::Str(_) | Literal::ByteStr(_), _) => {
                let value = text_value(literal).expect("a string literal");
                let index = self.promoted.len() as u32;
                self.promoted.push(value);
                Constant::Promoted {
                    index,
                    ty: self.expr_ty(expr),
                }
            }
            (Literal::CStr(bytes), _) => {
                self.c_strings.push(bytes.clone());
                Constant::CStr(self.c_strings.len() as u32 - 1)
            }
            _ => Constant::Unit,
        }
    }

    /// Evaluates `expr` and stores its value in `dest`, reborrowing it if
    /// type checking said so.
    fn lower_into(&mut self, expr: ExprId, dest: &Place) {
        // A function item coerced to a pointer: the item is evaluated,
        // for what its expression does, then its address taken.
        if self.types.reified.get(expr).is_some()
            && let TyKind::FnDef(def) = self.own_ty(expr).kind()
        {
            let item = self.temp(self.own_ty(expr));
            self.lower_value(expr, &item.into());
            let ty = self.place_ty(dest);
            let pointer = Constant::Fn {
                function: def.function,
                args: def.args,
                ty,
            };
            self.assign(dest, Rvalue::Use(Operand::Const(pointer)), self.range(expr));
            return;
        }
        // A closure that captures nothing coerced to a pointer: to its
        // `call_once`, which takes nothing for it.
        if self.types.reified.get(expr).is_some()
            && let TyKind::Closure(_, args) = self.own_ty(expr).kind()
            && let Some(once) = flux_ty::fn_trait(self.items, flux_ty::ClosureKind::FnOnce)
            && let Some(call_once) =
                self.items.traits[once].function(self.items, Symbol::intern("call_once"))
        {
            let closure = self.own_ty(expr);
            let item = self.temp(closure);
            self.lower_value(expr, &item.into());
            let ty = self.place_ty(dest);
            let params = flux_ty::ClosureArgs::split(args).params();
            let pointer = Constant::Fn {
                function: call_once,
                args: Ty::list(&[closure, params]),
                ty,
            };
            self.assign(dest, Rvalue::Use(Operand::Const(pointer)), self.range(expr));
            return;
        }
        // A reference to an array as one to a slice: the reference, then
        // its unsizing; a reference as a raw pointer, the same.
        if let Some(&ty) = self.types.unsizes.get(expr) {
            let reference = match self.types.reborrows.get(expr) {
                Some(&m) => {
                    let pointee = match self.expr_ty(expr).kind() {
                        TyKind::Ref(_, pointee) => pointee,
                        _ => Ty::ERROR,
                    };
                    Ty::new(TyKind::Ref(m, pointee))
                }
                None => self.expr_ty(expr),
            };
            let temp = self.temp(reference);
            self.lower_reborrowed(expr, &temp.into());
            let mut operand = self.read(temp.into());
            let to_object =
                matches!(ty.kind(), TyKind::Ref(_, p) | TyKind::RawPtr(_, p) if p.is_dyn());
            // A reference coerced to a raw pointer is cast to one, which
            // may then be unsized to one to a trait object.
            if let (TyKind::Ref(_, pointee), TyKind::RawPtr(m, _)) = (reference.kind(), ty.kind())
                && to_object
            {
                let raw = self.temp(Ty::raw_ptr(m, pointee));
                self.assign(
                    &raw.into(),
                    Rvalue::Cast(operand, Ty::raw_ptr(m, pointee)),
                    self.range(expr),
                );
                operand = self.read(raw.into());
            }
            let rvalue = if let TyKind::RawPtr(..) = ty.kind()
                && !to_object
            {
                Rvalue::Cast(operand, ty)
            } else {
                Rvalue::Unsize(operand, ty)
            };
            // At the value a block ends with, where rustc reports what
            // the coercion needs.
            let mut at = expr;
            while let Expr::Block {
                tail: Some(tail), ..
            } = self.body.exprs[at]
            {
                at = tail;
            }
            self.assign(dest, rvalue, self.range(at));
            return;
        }
        self.lower_reborrowed(expr, dest);
    }

    /// The value of `expr` if rustc would promote a borrow of it to a
    /// constant: a literal, or a tuple or array of them, of a type
    /// whose value needs no dropping.
    fn promotable(&self, expr: ExprId) -> Option<crate::eval::Value> {
        use crate::eval::Value;
        if self.types.reified.get(expr).is_some() || self.types.unsizes.get(expr).is_some() {
            return None;
        }
        let ty = self.own_ty(expr);
        match &self.body.exprs[expr] {
            Expr::Literal(Literal::Int { value, .. }) => Some(Value::Int {
                bits: Constant::int(*value as i128, ty.as_int()?).int_bits()?,
                ty: ty.as_int()?,
            }),
            Expr::Literal(Literal::Bool(b)) => Some(Value::Bool(*b)),
            Expr::Literal(Literal::Char(c)) => Some(Value::Int {
                bits: u128::from(*c),
                ty: IntTy::U32,
            }),
            Expr::Literal(literal @ (Literal::Str(_) | Literal::ByteStr(_))) => {
                Some(Value::Ref(Box::new(text_value(literal)?)))
            }
            Expr::Unary {
                op: UnaryOp::Neg,
                operand,
            } => match &self.body.exprs[*operand] {
                Expr::Literal(Literal::Int { value, .. }) => {
                    let int = ty.as_int()?;
                    Some(Value::Int {
                        bits: Constant::int((*value as i128).wrapping_neg(), int).int_bits()?,
                        ty: int,
                    })
                }
                _ => None,
            },
            Expr::Tuple(fields) | Expr::Array(fields) => {
                let values: Option<Vec<Value>> =
                    fields.iter().map(|&f| self.promotable(f)).collect();
                Some(Value::Aggregate(values?))
            }
            _ => None,
        }
    }

    /// Evaluates `expr` and stores its value in `dest`, reborrowing it if
    /// type checking said so.
    fn lower_reborrowed(&mut self, expr: ExprId, dest: &Place) {
        let Some(&mutability) = self.types.reborrows.get(expr) else {
            return self.lower_value(expr, dest);
        };
        // `&**r` for a deref coercion.
        let derefs = self.types.deref_coercions.get(expr).copied().unwrap_or(1);
        let place = if is_place(self.body, expr) {
            self.lower_place_deref(expr, derefs)
        } else {
            let temp = self.temp(self.expr_ty(expr));
            self.lower_value(expr, &temp.into());
            (0..derefs).fold(Place::from(temp), |place, _| place.deref())
        };
        let rvalue = Rvalue::Ref {
            mutability,
            place,
            two_phase: false,
        };
        // A block's is its value's, where rustc reports it.
        let mut at = expr;
        while let Expr::Block {
            tail: Some(tail), ..
        } = self.body.exprs[at]
        {
            at = tail;
        }
        self.assign(dest, rvalue, self.range(at));
    }

    /// The constant path `expr`, resolved as `res`, names: with its
    /// owner's generic arguments if it has any.
    fn const_of(&self, expr: ExprId, res: Res) -> Option<flux_ty::ConstInstance> {
        if let Some(&c) = self.types.consts.get(expr) {
            return Some(c);
        }
        match res {
            Res::Const(konst) => Some(flux_ty::ConstInstance { konst, args: &[] }),
            _ => None,
        }
    }

    /// Evaluates `expr` and stores its value in `dest`.
    fn lower_value(&mut self, expr: ExprId, dest: &Place) {
        let range = self.range(expr);
        let body = self.body;
        match &body.exprs[expr] {
            Expr::Path(Res::Const(_) | Res::ConstParam(_)) => {
                let operand = self.direct_operand(expr);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            Expr::Path(Res::Assoc { .. }) if self.types.consts.get(expr).is_some() => {
                let operand = self.direct_operand(expr);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            // A function item has no value but its type, a tuple
            // struct's or variant's constructor too.
            Expr::Path(
                Res::Function(_) | Res::Assoc { .. } | Res::Struct(_) | Res::Variant(..),
            ) if matches!(self.own_ty(expr).kind(), TyKind::FnDef(_)) => {
                let item = Constant::FnItem(self.own_ty(expr));
                self.assign(dest, Rvalue::Use(Operand::Const(item)), range);
            }
            Expr::Missing | Expr::Path(Res::Function(_) | Res::Assoc { .. } | Res::Err) => {
                self.bug("a missing or unresolved expression", range);
            }
            Expr::Literal(_) => {
                let operand = self.direct_operand(expr);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            Expr::Tuple(fields) => {
                let operands = fields.iter().map(|&f| self.lower_operand(f)).collect();
                self.assign(
                    dest,
                    Rvalue::Aggregate(AggregateKind::Tuple, operands),
                    range,
                );
            }
            Expr::Path(Res::Struct(strukt)) => {
                // Only unit structs type-check as values.
                self.assign(
                    dest,
                    Rvalue::Aggregate(AggregateKind::Struct(*strukt), Vec::new()),
                    range,
                );
            }
            Expr::Path(Res::Variant(strukt, variant)) => {
                // Only unit variants type-check as values.
                self.assign(
                    dest,
                    Rvalue::Aggregate(AggregateKind::Variant(*strukt, *variant), Vec::new()),
                    range,
                );
            }
            Expr::Let { .. } => {
                // Only in conditions, which lower them as branches.
                self.bug("`let` outside a condition", range);
            }
            Expr::Match { scrutinee, arms } => {
                self.lower_match(*scrutinee, arms, dest, range);
            }
            Expr::Closure { .. } => self.lower_closure(expr, dest, range),
            Expr::Path(Res::Local(binding))
                if self.binding_locals.get(*binding).is_none()
                    && self.guard_places.get(*binding).is_none()
                    && self.upvar_place(expr).is_none() =>
            {
                self.bug("a local neither declared nor captured", range);
            }
            Expr::Path(Res::Local(binding))
                if let Some(place) = self.guard_places.get(*binding).cloned()
                    && matches!(self.read(place), Operand::Move(_)) =>
            {
                let name = body.bindings[*binding].name;
                self.errors.push(flux_hir::Diagnostic::new(
                    range,
                    format!("cannot move out of `{name}` in pattern guard"),
                ));
                // Moves nothing, as in rustc, so later uses are not errors
                // too; the program does not compile.
                let place = self.lower_place(expr);
                self.assign(dest, Rvalue::Use(Operand::Copy(place)), range);
            }
            // Reads of places, including fields of values that are not
            // places: `make().x`.
            Expr::Path(Res::Local(_) | Res::Static(_))
            | Expr::Field { .. }
            | Expr::Index { .. }
            | Expr::Unary {
                op: UnaryOp::Deref, ..
            } => {
                let place = self.lower_place(expr);
                let operand = self.read(place);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            Expr::Ref {
                mutable,
                expr: inner,
            } => {
                if *mutable && is_place(body, *inner) {
                    self.check_guard_write(*inner, true, range);
                }
                // A constant expression borrowed is promoted to a value
                // that lives forever, as rustc does.
                if !*mutable && let Some(value) = self.promotable(*inner) {
                    let index = self.promoted.len() as u32;
                    self.promoted.push(value);
                    let ty = self.own_ty(expr);
                    let constant = Constant::Promoted { index, ty };
                    self.assign(dest, Rvalue::Use(Operand::Const(constant)), range);
                    return;
                }
                self.extending = self.extended_refs.contains(&expr);
                let place = self.lower_place(*inner);
                self.extending = false;
                let mutability = if *mutable {
                    Mutability::Mut
                } else {
                    Mutability::Not
                };
                let rvalue = Rvalue::Ref {
                    mutability,
                    place,
                    two_phase: false,
                };
                self.assign(dest, rvalue, range);
            }
            // A reference, at once cast to a raw pointer: the place is
            // accessed as a borrow of it is, but no loan outlives it.
            Expr::RawRef {
                mutable,
                expr: inner,
            } => {
                if *mutable {
                    self.check_guard_write(*inner, true, range);
                }
                let mutability = if *mutable {
                    Mutability::Mut
                } else {
                    Mutability::Not
                };
                let place = self.lower_place(*inner);
                let pointee = self.place_ty(&place);
                let temp = self.temp(Ty::new(TyKind::Ref(mutability, pointee)));
                let rvalue = Rvalue::Ref {
                    mutability,
                    place,
                    two_phase: false,
                };
                self.assign(&temp.into(), rvalue, range);
                let operand = self.read(temp.into());
                let ty = self.expr_ty(expr);
                self.assign(dest, Rvalue::Cast(operand, ty), range);
            }
            Expr::Record {
                strukt,
                variant,
                fields,
                spread,
            } => self.lower_record(*strukt, *variant, fields, *spread, dest, range),
            Expr::Array(elements) => {
                let elem = match self.expr_ty(expr).kind() {
                    TyKind::Array(elem, _) => elem,
                    _ => Ty::ERROR,
                };
                let operands = elements.iter().map(|&e| self.lower_operand(e)).collect();
                self.assign(
                    dest,
                    Rvalue::Aggregate(AggregateKind::Array(elem), operands),
                    range,
                );
            }
            Expr::Repeat { value, .. } => {
                let count = match self.expr_ty(expr).kind() {
                    TyKind::Array(_, count) => count,
                    _ => Ty::usize_value(0),
                };
                let operand = self.lower_operand(*value);
                self.assign(dest, Rvalue::Repeat(operand, count), range);
            }
            Expr::MethodCall { receiver, .. }
                if let Some(&(method, derefs)) = self.types.builtin_methods.get(expr) =>
            {
                let receiver_range = self.range(*receiver);
                let place = self.lower_place_deref(*receiver, derefs);
                let rvalue = Rvalue::Len {
                    place,
                    borrows: true,
                };
                match method {
                    flux_ty::BuiltinMethod::Len => self.assign(dest, rvalue, receiver_range),
                    flux_ty::BuiltinMethod::IsEmpty => {
                        let len = self.temp(Ty::int(IntTy::Usize));
                        self.assign(&len.into(), rvalue, receiver_range);
                        let zero = Operand::Const(Constant::int(0, IntTy::Usize));
                        let rvalue = Rvalue::BinaryOp(BinOp::Eq, Operand::Copy(len.into()), zero);
                        self.assign(dest, rvalue, range);
                    }
                }
            }
            Expr::Block { stmts, tail, .. } => {
                self.scoped(ScopeKind::Block, range, |b| {
                    for stmt in stmts.iter() {
                        b.scoped(ScopeKind::Temps, range, |b| b.lower_stmt(stmt));
                    }
                    match tail {
                        Some(tail) => {
                            b.scoped(ScopeKind::Temps, range, |b| b.lower_into(*tail, dest));
                        }
                        None => b.assign_unit(dest, range),
                    }
                });
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } if self.has_let(*condition) => {
                self.lower_if_let(*condition, *then_branch, *else_branch, dest, range);
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.scoped(ScopeKind::Temps, range, |b| b.lower_operand(*condition));
                let then_block = self.new_block();
                let else_block = self.new_block();
                let join = self.new_block();
                self.terminate(
                    TerminatorKind::SwitchInt {
                        discr: cond,
                        targets: vec![(0, else_block)],
                        otherwise: then_block,
                        variants: None,
                    },
                    range,
                );
                self.current = Some(then_block);
                self.lower_into(*then_branch, dest);
                self.goto(join, range);
                self.current = Some(else_block);
                match else_branch {
                    Some(else_branch) => self.lower_into(*else_branch, dest),
                    None => self.assign_unit(dest, range),
                }
                self.goto(join, range);
                self.current = Some(join);
            }
            Expr::Loop {
                body: loop_body, ..
            } => {
                let head = self.new_block();
                let exit = self.new_block();
                self.goto(head, range);
                self.current = Some(head);
                self.loops.insert(
                    expr,
                    LoopScope {
                        break_block: exit,
                        continue_block: head,
                        dest: dest.clone(),
                        scope_depth: self.scopes.len(),
                    },
                );
                self.lower_operand(*loop_body);
                self.loops.remove(expr);
                self.goto(head, range);
                self.current = Some(exit);
            }
            Expr::Break { value, target } => {
                let scope = target.and_then(|t| self.loops.get(t).cloned());
                let Some(scope) = scope else {
                    self.bug("`break` outside a loop", range);
                    return;
                };
                match value {
                    Some(value) => self.lower_into(*value, &scope.dest),
                    None => self.assign_unit(&scope.dest, range),
                }
                self.leave_scopes(scope.scope_depth, true, range);
                self.goto(scope.break_block, range);
            }
            Expr::Continue { target } => match target.and_then(|t| self.loops.get(t).cloned()) {
                Some(scope) => {
                    self.leave_scopes(scope.scope_depth, true, range);
                    self.goto(scope.continue_block, range);
                }
                None => self.bug("`continue` outside a loop", range),
            },
            Expr::Return { value } => {
                let ret = Body::RETURN.into();
                match value {
                    Some(value) => self.lower_into(*value, &ret),
                    None => self.assign_unit(&ret, range),
                }
                self.leave_scopes(0, false, range);
                self.terminate(TerminatorKind::Return, range);
            }
            Expr::Call { callee, args } if let Some(&method) = self.types.methods.get(expr) => {
                let receiver = self.adjusted_receiver(*callee, method);
                // The arguments, as the tuple the trait's method takes.
                let instance = self.types.calls[expr];
                let tupled = instance.args.get(1).copied().unwrap_or(Ty::UNIT);
                let tupled = self.normalize(tupled);
                let operands = args.iter().map(|&arg| self.lower_operand(arg)).collect();
                let tuple = self.temp(tupled);
                self.assign(
                    &tuple.into(),
                    Rvalue::Aggregate(AggregateKind::Tuple, operands),
                    range,
                );
                let operands = vec![receiver, Operand::Move(tuple.into())];
                self.call(instance.function, instance.args, operands, dest, range);
            }
            Expr::Call { callee, args } => match body.exprs[*callee] {
                Expr::Path(Res::Function(_) | Res::Assoc { .. }) => {
                    let args = args.iter().map(|&arg| self.lower_operand(arg)).collect();
                    let Some(&instance) = self.types.calls.get(*callee) else {
                        self.bug("a call of a function not resolved", range);
                        return;
                    };
                    self.call(instance.function, instance.args, args, dest, range);
                }
                Expr::Path(Res::Struct(strukt)) => {
                    let operands = args.iter().map(|&arg| self.lower_operand(arg)).collect();
                    self.assign(
                        dest,
                        Rvalue::Aggregate(AggregateKind::Struct(strukt), operands),
                        range,
                    );
                }
                Expr::Path(Res::Variant(strukt, variant)) => {
                    let operands = args.iter().map(|&arg| self.lower_operand(arg)).collect();
                    self.assign(
                        dest,
                        Rvalue::Aggregate(AggregateKind::Variant(strukt, variant), operands),
                        range,
                    );
                }
                // A value of a function item's or pointer's type.
                _ => match self.expr_ty(*callee).kind() {
                    TyKind::FnDef(def) => {
                        // Evaluated for what it does; it holds nothing.
                        self.lower_operand(*callee);
                        let args = args.iter().map(|&arg| self.lower_operand(arg)).collect();
                        self.call(def.function, def.args, args, dest, range);
                    }
                    TyKind::FnPtr(sig) => {
                        let callee = self.lower_operand(*callee);
                        let args = args.iter().map(|&arg| self.lower_operand(arg)).collect();
                        let target = (sig.ret() != Ty::NEVER).then(|| self.new_block());
                        self.terminate(
                            TerminatorKind::CallIndirect {
                                callee,
                                args,
                                dest: dest.clone(),
                                target,
                            },
                            range,
                        );
                        self.current = target;
                    }
                    _ => self.bug("a call of a value that is not callable", range),
                },
            },
            Expr::MethodCall { receiver, args, .. }
                if let Some(&method) = self.types.methods.get(expr) =>
            {
                let receiver = self.adjusted_receiver(*receiver, method);
                let mut operands = vec![receiver];
                operands.extend(args.iter().map(|&arg| self.lower_operand(arg)));
                let instance = self.types.calls[expr];
                if let (Some(block), Some(name)) =
                    (self.current, self.source_map.expr_fields.get(expr))
                    && let Some(name) = name.first()
                {
                    self.fn_spans.insert(block, name.start());
                }
                self.call(instance.function, instance.args, operands, dest, range);
            }
            Expr::MethodCall { receiver, args, .. } => {
                // Only `CStr::as_ptr` type-checks: the reference is the
                // pointer.
                let receiver = self.lower_operand(*receiver);
                for &arg in args.iter() {
                    self.lower_operand(arg);
                }
                let ty = self.place_ty(dest);
                self.assign(dest, Rvalue::Cast(receiver, ty), range);
            }
            Expr::Unary { .. } | Expr::Binary { .. }
                if let Some(&instance) = self.types.overloads.get(expr) =>
            {
                self.lower_overloaded(expr, instance, dest, range);
            }
            Expr::Unary {
                op: UnaryOp::Neg,
                operand: inner,
            } if matches!(body.exprs[*inner], Expr::Literal(Literal::Int { .. })) => {
                let operand = self.lower_operand(expr);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            Expr::Unary { op, operand } => {
                let operand = self.lower_operand(*operand);
                let op = match op {
                    UnaryOp::Neg => UnOp::Neg,
                    _ => UnOp::Not,
                };
                self.assign(dest, Rvalue::UnaryOp(op, operand), range);
            }
            Expr::Binary { op, lhs, rhs } => match op {
                BinaryOp::Logic(logic) => self.lower_logic(*logic, *lhs, *rhs, dest, range),
                BinaryOp::Arith(arith) => {
                    let lhs_ty = self.expr_ty(*lhs);
                    let l = self.lower_operand(*lhs);
                    let r = self.lower_operand(*rhs);
                    let rvalue = self.arith(*arith, l, r, lhs_ty, range);
                    self.assign(dest, rvalue, range);
                }
                BinaryOp::Cmp(cmp) => {
                    let l = self.lower_operand(*lhs);
                    let r = self.lower_operand(*rhs);
                    let op = match cmp {
                        CmpOp::Eq => BinOp::Eq,
                        CmpOp::Ne => BinOp::Ne,
                        CmpOp::Lt => BinOp::Lt,
                        CmpOp::Le => BinOp::Le,
                        CmpOp::Gt => BinOp::Gt,
                        CmpOp::Ge => BinOp::Ge,
                    };
                    self.assign(dest, Rvalue::BinaryOp(op, l, r), range);
                }
                BinaryOp::Assign(arith) => {
                    self.lower_assign(*arith, *lhs, *rhs, range);
                    self.assign_unit(dest, range);
                }
            },
            Expr::Try { expr: operand } => self.lower_try(expr, *operand, dest, range),
            Expr::Cast { expr: inner, .. } => {
                let ty = self.expr_ty(expr);
                // A cast to the operand's own type, or to the type it is
                // coerced to, is the operand.
                if self.value_ty(*inner) == ty {
                    self.lower_into(*inner, dest);
                    return;
                }
                // A unit variant's path is its discriminant, as in rustc,
                // which needs the value of that variant's discriminant only;
                // other values of a fieldless enum are cast through it.
                let operand =
                    if let Expr::Path(Res::Variant(strukt, variant)) = self.body.exprs[*inner] {
                        self.variant_discriminant(strukt, variant, range)
                    } else if matches!(self.expr_ty(*inner).kind(), TyKind::Adt(..)) {
                        // The cast takes the value: moves it unless it is `Copy`.
                        let mut place = self.lower_place(*inner);
                        if let Operand::Move(_) = self.read(place.clone()) {
                            let value = self.temp(self.expr_ty(*inner));
                            self.assign(&value.into(), Rvalue::Use(Operand::Move(place)), range);
                            place = value.into();
                        }
                        let int = self.typed.sigs.discriminant_ty(self.expr_ty(*inner));
                        let discriminant = self.temp(Ty::int(int.unwrap_or(IntTy::Isize)));
                        self.assign(&discriminant.into(), Rvalue::Discriminant(place), range);
                        Operand::Copy(discriminant.into())
                    } else {
                        self.lower_operand(*inner)
                    };
                self.assign(dest, Rvalue::Cast(operand, ty), range);
            }
        }
    }

    /// An operator on a type other than the integers and `bool`: a call
    /// of its trait's method, `instance`, with the operands by value, by
    /// reference for comparisons (`PartialEq::eq(&a, &b)`), or as they
    /// are for comparisons of references, whose referents are compared,
    /// and the left one by mutable reference for compound assignments.
    fn lower_overloaded(
        &mut self,
        expr: ExprId,
        instance: flux_ty::Instance,
        dest: &Place,
        range: TextRange,
    ) {
        let args = match self.body.exprs[expr] {
            Expr::Unary { operand, .. } => vec![self.lower_operand(operand)],
            Expr::Binary {
                op: BinaryOp::Cmp(cmp),
                lhs,
                rhs,
            } if self.types.compared_refs.get(expr).is_some() => {
                if !matches!(cmp, CmpOp::Eq | CmpOp::Ne)
                    && let TyKind::Ref(Mutability::Mut, _) = self.expr_ty(lhs).kind()
                {
                    self.mut_ordered_operands(lhs, rhs)
                } else {
                    vec![self.shared_operand(lhs), self.shared_operand(rhs)]
                }
            }
            Expr::Binary {
                op: BinaryOp::Cmp(_),
                lhs,
                rhs,
            } => vec![
                self.borrow(lhs, Mutability::Not),
                self.borrow(rhs, Mutability::Not),
            ],
            Expr::Binary {
                op: BinaryOp::Assign(_),
                lhs,
                rhs,
            } => {
                let lhs = self.borrow(lhs, Mutability::Mut);
                vec![lhs, self.lower_operand(rhs)]
            }
            Expr::Binary { lhs, rhs, .. } => {
                vec![self.lower_operand(lhs), self.lower_operand(rhs)]
            }
            _ => unreachable!("only operators are overloaded"),
        };
        self.call(instance.function, instance.args, args, dest, range);
    }

    /// `expr`, a reference, as a shared one: a `&mut` is reborrowed
    /// (`&*r`), as rustc's `impl PartialEq<&mut B> for &A` takes it by
    /// reference rather than moving it.
    fn shared_operand(&mut self, expr: ExprId) -> Operand {
        let TyKind::Ref(Mutability::Mut, pointee) = self.expr_ty(expr).kind() else {
            return self.lower_operand(expr);
        };
        let range = self.range(expr);
        let place = self.lower_place_deref(expr, 1);
        let temp = self.temp(Ty::new(TyKind::Ref(Mutability::Not, pointee)));
        let rvalue = Rvalue::Ref {
            mutability: Mutability::Not,
            place,
            two_phase: false,
        };
        self.assign(&temp.into(), rvalue, range);
        Operand::Copy(temp.into())
    }

    /// The operands of `a < b` where `a` and `b` are `&mut` references:
    /// as rustc's `PartialOrd::lt(&a, &mut *b)`, which borrows `a` and
    /// reborrows `b` mutably, so `m < m` is E0502; then shared
    /// references to what they point to, which `core`'s impl for `&mut`
    /// would pass on.
    fn mut_ordered_operands(&mut self, lhs: ExprId, rhs: ExprId) -> Vec<Operand> {
        let place = self.lower_place(lhs);
        let ty = Ty::new(TyKind::Ref(Mutability::Not, self.place_ty(&place)));
        let l = self.temp(ty);
        let rvalue = Rvalue::Ref {
            mutability: Mutability::Not,
            place,
            two_phase: false,
        };
        self.assign(&l.into(), rvalue, self.range(lhs));
        let place = self.lower_place_deref(rhs, 1);
        let ty = Ty::new(TyKind::Ref(Mutability::Mut, self.place_ty(&place)));
        let r = self.temp(ty);
        let rvalue = Rvalue::Ref {
            mutability: Mutability::Mut,
            place,
            two_phase: false,
        };
        self.assign(&r.into(), rvalue, self.range(rhs));
        [
            (Place::from(l).deref().deref(), lhs),
            (Place::from(r).deref(), rhs),
        ]
        .into_iter()
        .map(|(place, expr)| {
            let ty = Ty::new(TyKind::Ref(Mutability::Not, self.place_ty(&place)));
            let temp = self.temp(ty);
            let rvalue = Rvalue::Ref {
                mutability: Mutability::Not,
                place,
                two_phase: false,
            };
            self.assign(&temp.into(), rvalue, self.range(expr));
            Operand::Copy(temp.into())
        })
        .collect()
    }

    /// `operand?`: moves the `Option` or `Result` into a temporary, as
    /// rustc's `Try::branch(operand)` takes it, and tests its variant.
    /// The `Some` or `Ok` value goes to `dest`; otherwise the function
    /// returns `None`, or `Err` with the error, converted by `From::from`
    /// if type checking said so, dropping what is in scope as `return`
    /// does. Either way the temporary is moved out of, so it needs no
    /// drop.
    fn lower_try(&mut self, expr: ExprId, operand: ExprId, dest: &Place, range: TextRange) {
        let ty = self.expr_ty(operand);
        let TyKind::Adt(adt, _) = ty.kind() else {
            self.bug("`?` on neither `Option` nor `Result`", range);
            return;
        };
        let strukt = adt.strukt;
        let lang = |name: &str| self.items.lang_enums.get(&Symbol::intern(name)) == Some(&strukt);
        let option = lang("Option");
        let (value, residual) = if option {
            ("Some", "None")
        } else if lang("ControlFlow") {
            ("Continue", "Break")
        } else {
            ("Ok", "Err")
        };
        let data = &self.items.structs[strukt];
        let (Some(value), Some(residual)) = (
            data.variant_named(Symbol::intern(value)),
            data.variant_named(Symbol::intern(residual)),
        ) else {
            self.bug("`?` without its variants", range);
            return;
        };
        let temp = self.temp(ty);
        self.lower_into(operand, &temp.into());
        let fail = self.new_block();
        let place = Place::from(temp).into();
        let place = self.test_variant(strukt, value, place, fail, range);
        let place = self.resolve(place, range);
        let output = self.read(place.field(0));
        self.assign(dest, Rvalue::Use(output), range);
        let next = self.current;
        self.current = Some(fail);
        let fields = if option {
            Vec::new()
        } else {
            let error = self.read(Place::from(temp).downcast(residual).field(0));
            match self.types.try_conversions.get(expr) {
                Some(&instance) => {
                    let converted = self.temp(instance.args[0]);
                    self.call(
                        instance.function,
                        instance.args,
                        vec![error],
                        &converted.into(),
                        range,
                    );
                    vec![self.read(converted.into())]
                }
                None => vec![error],
            }
        };
        let rvalue = Rvalue::Aggregate(AggregateKind::Variant(strukt, residual), fields);
        self.assign(&Body::RETURN.into(), rvalue, range);
        self.leave_scopes(0, false, range);
        self.terminate(TerminatorKind::Return, range);
        self.current = next;
    }

    /// A reference to what `expr` denotes, in a temporary.
    fn borrow(&mut self, expr: ExprId, mutability: Mutability) -> Operand {
        let range = self.range(expr);
        let place = self.lower_place(expr);
        let temp = self.temp(Ty::new(TyKind::Ref(mutability, self.place_ty(&place))));
        let rvalue = Rvalue::Ref {
            mutability,
            place,
            two_phase: mutability == Mutability::Mut,
        };
        self.assign(&temp.into(), rvalue, range);
        self.read(temp.into())
    }

    /// Calls `func` with generic arguments `generic_args`, storing the
    /// result in `dest`.
    fn call(
        &mut self,
        func: FunctionId,
        generic_args: GenericArgs,
        args: Vec<Operand>,
        dest: &Place,
        range: TextRange,
    ) {
        let returns = self.typed.sigs[func].ret.subst(generic_args) != Ty::NEVER;
        let target = returns.then(|| self.new_block());
        self.terminate(
            TerminatorKind::Call {
                func,
                generic_args,
                args,
                dest: dest.clone(),
                target,
            },
            range,
        );
        self.current = target;
    }

    /// `S { a: 1, b, ..base }`: the given fields in source order, then
    /// the rest read from `base`, which is moved out of only for fields
    /// that are not `Copy`.
    fn lower_record(
        &mut self,
        strukt: StructId,
        variant: u32,
        fields: &[(Symbol, ExprId)],
        spread: Option<ExprId>,
        dest: &Place,
        range: TextRange,
    ) {
        let count = self.items.structs[strukt].variant(variant).fields.len();
        let mut operands: Vec<Option<Operand>> = vec![None; count];
        for &(name, value) in fields {
            let operand = self.lower_operand(value);
            operands[self.struct_field(strukt, variant, name) as usize] = Some(operand);
        }
        if let Some(base) = spread {
            let base = self.lower_place(base);
            for (i, operand) in operands.iter_mut().enumerate() {
                if operand.is_none() {
                    *operand = Some(self.read(base.field(i as u32)));
                }
            }
        }
        let operands = operands
            .into_iter()
            .map(|o| o.unwrap_or(Operand::Const(Constant::Unit)))
            .collect();
        let kind = self.aggregate(strukt, variant);
        self.assign(dest, Rvalue::Aggregate(kind, operands), range);
    }

    fn lower_stmt(&mut self, stmt: &Stmt) {
        match *stmt {
            Stmt::Let {
                pat,
                init: Some(init),
                else_branch: Some(else_branch),
                ..
            } => {
                self.declare_bindings(pat);
                self.lower_let_else(pat, init, else_branch);
            }
            Stmt::Let { pat, init, .. } => {
                let scope = self
                    .scopes
                    .iter()
                    .rposition(|s| s.kind != ScopeKind::Temps)
                    .expect("bodies have a scope");
                let at = self.scopes[scope].locals.len();
                self.declare_bindings(pat);
                let Some(init) = init else {
                    return;
                };
                let mut extended = Vec::new();
                self.extending_refs(init, &mut extended);
                let saved = (
                    std::mem::replace(&mut self.extended_refs, extended),
                    self.extend_at.replace((scope, at)),
                );
                self.lower_let(pat, init);
                (self.extended_refs, self.extend_at) = saved;
            }
            Stmt::Expr { expr, .. } => {
                // A value no one uses is dropped at the end of the
                // statement.
                if self.needs_drop(self.expr_ty(expr)) {
                    self.temp_for(expr);
                } else {
                    self.lower_operand(expr);
                }
            }
        }
    }

    /// The borrows in `expr` whose temporaries live as long as a `let`'s
    /// bindings when `expr` is its initializer: those it evaluates to, or
    /// to a tuple, struct or borrow of, as rustc extends them.
    fn extending_refs(&self, expr: ExprId, out: &mut Vec<ExprId>) {
        match &self.body.exprs[expr] {
            Expr::Ref { expr: inner, .. } => {
                out.push(expr);
                self.extended_operand(*inner, out);
            }
            Expr::Tuple(fields) | Expr::Array(fields) => {
                for &field in fields.iter() {
                    self.extending_refs(field, out);
                }
            }
            Expr::Record { fields, .. } => {
                for &(_, field) in fields.iter() {
                    self.extending_refs(field, out);
                }
            }
            Expr::Block {
                tail: Some(tail), ..
            } => self.extending_refs(*tail, out),
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.extending_refs(*then_branch, out);
                if let Some(else_branch) = else_branch {
                    self.extending_refs(*else_branch, out);
                }
            }
            Expr::Match { arms, .. } => {
                for arm in arms.iter() {
                    self.extending_refs(arm.body, out);
                }
            }
            Expr::Cast { expr: inner, .. } => self.extending_refs(*inner, out),
            _ => {}
        }
    }

    /// `let pat = init;`.
    /// The borrows in the operand of an extended borrow, which are
    /// extended too, through dereferences and fields: `&*&make()`,
    /// `&(&make()).0`, but not `&{ &make() }.0`.
    fn extended_operand(&self, expr: ExprId, out: &mut Vec<ExprId>) {
        match &self.body.exprs[expr] {
            Expr::Unary {
                op: UnaryOp::Deref,
                operand: inner,
            }
            | Expr::Field { expr: inner, .. }
            | Expr::Index { base: inner, .. } => self.extended_operand(*inner, out),
            Expr::Ref { expr: inner, .. } => {
                out.push(expr);
                self.extended_operand(*inner, out);
            }
            _ => {}
        }
    }

    fn lower_let(&mut self, pat: PatId, init: ExprId) {
        match self.body.pats[pat] {
            Pat::Bind { binding, sub: None } if self.types.binding_refs.get(binding).is_none() => {
                let local = self.binding_locals[binding];
                self.lower_into(init, &local.into());
            }
            // `let _ = place;` does not read the place, nor does a
            // closure capture it for that.
            Pat::Wild if is_place(self.body, init) => {
                let PatPlace::Place(place) = self.pat_place(init) else {
                    return;
                };
                let range = self.range(init);
                let block = self.block();
                self.blocks[block.index()].0.push(Statement {
                    kind: StatementKind::PlaceMention(place),
                    range,
                });
            }
            _ => {
                let place = self.pat_place(init);
                self.match_irrefutable(pat, place, self.range(init));
            }
        }
    }

    /// `let pat = init else { .. };`: the bindings if `init` matches,
    /// else the block, which diverges.
    fn lower_let_else(&mut self, pat: PatId, init: ExprId, else_branch: ExprId) {
        let range = self.range(init);
        let place = self.pat_place(init);
        let otherwise = self.new_block();
        let matched = self.new_block();
        for exit in self.test(pat, place, otherwise, range) {
            self.current = Some(exit.block);
            self.bind_all(&exit.binds, range);
            self.goto(matched, range);
        }
        // The initializer's temporaries are dropped before the `else`
        // block runs, as in rustc.
        self.current = Some(otherwise);
        let temps = self.scopes.len() - 1;
        self.leave_scopes(temps, false, range);
        self.lower_operand(else_branch);
        if self.current.is_some() {
            self.terminate(TerminatorKind::Unreachable, range);
        }
        self.current = Some(matched);
    }

    /// Binds the bindings of `pat`, which matches the value in `place`
    /// whatever it is.
    fn match_irrefutable(&mut self, pat: PatId, place: PatPlace, range: TextRange) {
        if !self.tests(pat) {
            // Nothing to test, so nowhere to fail to.
            let here = self.block();
            for exit in self.test(pat, place, here, range) {
                self.bind_all(&exit.binds, range);
            }
            return;
        }
        let never = self.new_block();
        let matched = self.new_block();
        for exit in self.test(pat, place, never, range) {
            self.current = Some(exit.block);
            self.bind_all(&exit.binds, range);
            self.goto(matched, range);
        }
        self.current = Some(never);
        self.terminate(TerminatorKind::Unreachable, range);
        self.current = Some(matched);
    }

    /// Whether matching `pat` tests anything: which variant an enum
    /// holds, or what a value is.
    fn tests(&self, pat: PatId) -> bool {
        match &self.body.pats[pat] {
            Pat::Expr(_) | Pat::Range { .. } | Pat::Or(_) => true,
            Pat::TupleStruct { strukt, .. } | Pat::Record { strukt, .. } | Pat::Unit(strukt, _)
                if self.items.structs[*strukt].variant_count() > 1 =>
            {
                true
            }
            data => data.children().any(|child| self.tests(child)),
        }
    }

    /// A local for each binding in `pat`, in the innermost block's scope.
    fn declare_bindings(&mut self, pat: PatId) {
        let mut bindings = Vec::new();
        self.binding_pats(pat, &mut bindings);
        for (binding, range) in bindings {
            // A guard lowered once per way of matching declares its
            // `let`'s bindings once.
            if self.binding_locals.get(binding).is_some() {
                continue;
            }
            let data = &self.body.bindings[binding];
            let ty = self
                .types
                .binding_types
                .get(binding)
                .copied()
                .unwrap_or(Ty::ERROR);
            let local = self.locals.alloc(LocalDecl {
                ty,
                kind: LocalKind::User {
                    name: data.name,
                    mutable: data.mutable,
                },
            });
            self.binding_locals.insert(binding, local);
            self.binding_ranges.insert(binding, range);
            let scope = self
                .scopes
                .iter_mut()
                .rev()
                .find(|s| s.kind != ScopeKind::Temps)
                .expect("bodies have a scope");
            scope.locals.push((local, range));
        }
    }

    /// The bindings in `pat`, each once with its first pattern's range.
    fn binding_pats(&self, pat: PatId, out: &mut Vec<(BindingId, TextRange)>) {
        if let Pat::Bind { binding, .. } = self.body.pats[pat]
            && !out.iter().any(|&(b, _)| b == binding)
        {
            out.push((binding, self.pat_range(pat)));
        }
        for child in self.body.pats[pat].children() {
            self.binding_pats(child, out);
        }
    }

    /// Tests whether the value in `place` matches `pat`, going to `fail`
    /// if it does not, and gives where control goes if it does: one exit
    /// per way of matching, more than one for an or-pattern, each with
    /// the places its bindings bind. Tests read only the parts of the
    /// place they look at, at `range`, the scrutinee.
    fn test(
        &mut self,
        pat: PatId,
        place: PatPlace,
        fail: BasicBlock,
        range: TextRange,
    ) -> Vec<Exit> {
        let mut place = place;
        for _ in 0..self.types.pat_derefs.get(pat).copied().unwrap_or(0) {
            place = place.deref();
        }
        let here = |b: &mut Self| {
            vec![Exit {
                block: b.block(),
                binds: Vec::new(),
                resume: fail,
            }]
        };
        let body = self.body;
        match &body.pats[pat] {
            Pat::Missing | Pat::Wild => here(self),
            Pat::Bind { binding, sub } => {
                let mut exits = match sub {
                    Some(sub) => self.test(*sub, place.clone(), fail, range),
                    None => here(self),
                };
                // As rustc, after the subpattern's: `x @ Some(y)` moves
                // `y` out before `x` would take the whole.
                let place = self.resolve(place, range);
                for exit in &mut exits {
                    exit.binds.push((*binding, place.clone()));
                }
                exits
            }
            Pat::Tuple { pats, rest } => {
                let count = self
                    .typed
                    .sigs
                    .fields(self.pat_place_ty(&place))
                    .map_or(0, |f| f.len());
                let parts = positions(pats, *rest, count)
                    .map(|(i, p)| (p, place.field(i)))
                    .collect();
                self.test_all(parts, fail, range)
            }
            Pat::TupleStruct {
                strukt,
                variant,
                pats,
                rest,
            } => {
                let place = self.test_variant(*strukt, *variant, place, fail, range);
                let count = self.items.structs[*strukt].variant(*variant).fields.len();
                let parts = positions(pats, *rest, count)
                    .map(|(i, p)| (p, place.field(i)))
                    .collect();
                self.test_all(parts, fail, range)
            }
            Pat::Record {
                strukt,
                variant,
                fields,
                ..
            } => {
                let place = self.test_variant(*strukt, *variant, place, fail, range);
                let parts = fields
                    .iter()
                    .map(|&(name, p)| (p, place.field(self.struct_field(*strukt, *variant, name))))
                    .collect();
                self.test_all(parts, fail, range)
            }
            Pat::Unit(strukt, variant) => {
                self.test_variant(*strukt, *variant, place, fail, range);
                here(self)
            }
            Pat::Expr(expr) => {
                let place = self.resolve(place, range);
                if let Expr::Literal(Literal::Bool(value)) = body.exprs[*expr] {
                    let ok = self.new_block();
                    let (on_false, otherwise) = if value { (fail, ok) } else { (ok, fail) };
                    self.terminate(
                        TerminatorKind::SwitchInt {
                            discr: Operand::Copy(place),
                            targets: vec![(0, on_false)],
                            otherwise,
                            variants: None,
                        },
                        range,
                    );
                    self.current = Some(ok);
                } else if let Some(&eq) = self.types.overloads.get(*expr) {
                    // A string is compared by `<str as PartialEq>::eq`, as
                    // rustc's MIR does.
                    let value = self.lower_operand(*expr);
                    self.str_eq_test(eq, place, value, fail, range);
                } else {
                    let value = self.lower_operand(*expr);
                    self.compare(BinOp::Eq, place, value, fail, range);
                }
                here(self)
            }
            Pat::Range {
                start,
                end,
                inclusive,
            } => {
                let place = self.resolve(place, range);
                if let Some(start) = start {
                    let value = self.lower_operand(*start);
                    self.compare(BinOp::Ge, place.clone(), value, fail, range);
                }
                if let Some(end) = end {
                    let value = self.lower_operand(*end);
                    let op = if *inclusive { BinOp::Le } else { BinOp::Lt };
                    self.compare(op, place, value, fail, range);
                }
                here(self)
            }
            Pat::Or(alternatives) => {
                let mut exits = Vec::new();
                for (i, &alt) in alternatives.iter().enumerate() {
                    let last = i + 1 == alternatives.len();
                    let next = if last { fail } else { self.new_block() };
                    exits.extend(self.test(alt, place.clone(), next, range));
                    self.current = (!last).then_some(next);
                }
                exits
            }
            Pat::Ref { pat: inner, .. } => self.test(*inner, place.deref(), fail, range),
            Pat::Slice {
                prefix,
                slice,
                suffix,
            } => self.test_slice(prefix, *slice, suffix, place, fail, range),
        }
    }

    /// `[a, b, .., z]` on an array or slice in `place`: a slice's length
    /// is tested first (`== 3`, or `>= 3` with `..`), then the elements,
    /// each at a constant index from the start or, after `..`, from the
    /// end. `rest @ ..` binds an array of the elements it stands for, or
    /// a reference to the part of a slice.
    fn test_slice(
        &mut self,
        prefix: &[PatId],
        slice: Option<PatId>,
        suffix: &[PatId],
        place: PatPlace,
        fail: BasicBlock,
        range: TextRange,
    ) -> Vec<Exit> {
        let place = self.resolve(place, range);
        let usize = IntTy::Usize;
        let (p, s) = (prefix.len() as i128, suffix.len() as i128);
        // The length: a constant for an array, read for a slice.
        let (elem, len) = match self.place_ty(&place).kind() {
            TyKind::Array(elem, n) => (
                elem,
                Operand::Const(Constant::int(n.as_usize().unwrap_or(0) as i128, usize)),
            ),
            TyKind::Slice(elem) => {
                let len = self.temp(Ty::int(usize));
                self.assign(
                    &len.into(),
                    Rvalue::Len {
                        place: place.clone(),
                        borrows: false,
                    },
                    range,
                );
                let op = if slice.is_some() {
                    BinOp::Ge
                } else {
                    BinOp::Eq
                };
                self.compare(
                    op,
                    len.into(),
                    Operand::Const(Constant::int(p + s, usize)),
                    fail,
                    range,
                );
                (elem, Operand::Copy(len.into()))
            }
            _ => (Ty::ERROR, Operand::Const(Constant::int(0, usize))),
        };
        // `place[i]`, `i` in a temporary.
        let index = |b: &mut Self, value: Rvalue| {
            let at = b.temp(Ty::int(usize));
            b.assign(&at.into(), value, range);
            PatPlace::Place(place.project(ProjectionElem::Index(at)))
        };
        let mut parts = Vec::new();
        for (i, &pat) in prefix.iter().enumerate() {
            let at = index(
                self,
                Rvalue::Use(Operand::Const(Constant::int(i as i128, usize))),
            );
            parts.push((pat, at));
        }
        for (j, &pat) in suffix.iter().enumerate() {
            let back = Operand::Const(Constant::int(s - j as i128, usize));
            let at = index(self, Rvalue::BinaryOp(BinOp::Sub, len.clone(), back));
            parts.push((pat, at));
        }
        let mut exits = self.test_all(parts, fail, range);
        let Some(rest) = slice else {
            return exits;
        };
        let Pat::Bind { binding, .. } = self.body.pats[rest] else {
            return exits;
        };
        // The rest, once the elements matched: its own place.
        let mut bound = Vec::new();
        for mut exit in exits.drain(..) {
            self.current = Some(exit.block);
            let rest_place = match self.place_ty(&place).kind() {
                TyKind::Array(_, n) => {
                    let n = n.as_usize().unwrap_or(0) as i128;
                    let count = n.saturating_sub(p + s);
                    let ty = Ty::array_of(elem, Ty::usize_value(count as u64));
                    let elements = (0..count)
                        .map(|i| {
                            let at = index(
                                self,
                                Rvalue::Use(Operand::Const(Constant::int(p + i, usize))),
                            );
                            let PatPlace::Place(at) = at else {
                                unreachable!()
                            };
                            self.read(at)
                        })
                        .collect();
                    let temp = self.temp(ty);
                    self.assign(
                        &temp.into(),
                        Rvalue::Aggregate(AggregateKind::Array(elem), elements),
                        range,
                    );
                    Place::from(temp)
                }
                _ => {
                    // `&*subslice(&place, p, len - s)`.
                    let whole = self.temp(Ty::new(TyKind::Ref(Mutability::Not, Ty::slice(elem))));
                    self.assign(
                        &whole.into(),
                        Rvalue::Ref {
                            mutability: Mutability::Not,
                            place: place.clone(),
                            two_phase: false,
                        },
                        range,
                    );
                    let end = self.temp(Ty::int(usize));
                    self.assign(
                        &end.into(),
                        Rvalue::BinaryOp(
                            BinOp::Sub,
                            len.clone(),
                            Operand::Const(Constant::int(s, usize)),
                        ),
                        range,
                    );
                    let part = self.temp(Ty::new(TyKind::Ref(Mutability::Not, Ty::slice(elem))));
                    match self
                        .items
                        .lang_fns
                        .get(&Symbol::intern("subslice"))
                        .copied()
                    {
                        Some(subslice) => {
                            let args = vec![
                                Operand::Move(whole.into()),
                                Operand::Const(Constant::int(p, usize)),
                                Operand::Copy(end.into()),
                            ];
                            self.call(subslice, Ty::list(&[elem]), args, &part.into(), range);
                        }
                        None => self.bug("no `subslice` intrinsic", range),
                    }
                    Place::from(part).deref()
                }
            };
            exit.block = self.block();
            exit.binds.push((binding, rest_place));
            bound.push(exit);
        }
        bound
    }

    /// Tests the patterns of the parts of a value in turn. A part that
    /// fails resumes the alternatives of an or-pattern before it, so
    /// every way of matching is tried, in order.
    fn test_all(
        &mut self,
        parts: Vec<(PatId, PatPlace)>,
        fail: BasicBlock,
        range: TextRange,
    ) -> Vec<Exit> {
        let mut exits = vec![Exit {
            block: self.block(),
            binds: Vec::new(),
            resume: fail,
        }];
        for (pat, place) in parts {
            let mut next = Vec::new();
            for exit in exits {
                self.current = Some(exit.block);
                for sub in self.test(pat, place.clone(), exit.resume, range) {
                    let mut binds = exit.binds.clone();
                    binds.extend(sub.binds);
                    next.push(Exit {
                        block: sub.block,
                        binds,
                        resume: sub.resume,
                    });
                }
            }
            exits = next;
        }
        exits
    }

    /// Tests that the enum in `place` holds variant `variant`; gives the
    /// place of that variant, or `place` itself for a struct.
    fn test_variant(
        &mut self,
        strukt: StructId,
        variant: u32,
        place: PatPlace,
        fail: BasicBlock,
        range: TextRange,
    ) -> PatPlace {
        let data = &self.items.structs[strukt];
        if !data.is_enum() {
            return place;
        }
        // The fields of an enum's variant are captured with the enum.
        let place = self.resolve(place, range);
        if data.variant_count() > 1 {
            let int = flux_ty::discriminant_ty(self.items, strukt);
            let discriminant = self.temp(Ty::int(int));
            self.assign(
                &discriminant.into(),
                Rvalue::Discriminant(place.clone()),
                range,
            );
            let ok = self.new_block();
            self.terminate(
                TerminatorKind::SwitchInt {
                    discr: Operand::Copy(discriminant.into()),
                    targets: vec![(u128::from(variant), ok)],
                    otherwise: fail,
                    variants: Some(strukt),
                },
                range,
            );
            self.current = Some(ok);
        }
        PatPlace::Place(place.downcast(variant))
    }

    /// The discriminant of variant `variant` of enum `strukt`: a literal's
    /// value, or the constant of an expression plus the variants since.
    fn variant_discriminant(
        &mut self,
        strukt: StructId,
        variant: u32,
        range: TextRange,
    ) -> Operand {
        let int = flux_ty::discriminant_ty(self.items, strukt);
        let variants = self.items.structs[strukt]
            .variants
            .as_ref()
            .expect("an enum");
        match variants[variant as usize].discriminant {
            flux_hir::Discriminant::Value(value) => Operand::Const(Constant::int(value, int)),
            flux_hir::Discriminant::Const { konst, offset } => {
                let value = Operand::Const(Constant::Item {
                    id: konst,
                    args: Ty::list(&[]),
                    ty: Ty::int(int),
                });
                if offset == 0 {
                    return value;
                }
                let sum = self.temp(Ty::int(int));
                let offset = Operand::Const(Constant::int(i128::from(offset), int));
                self.assign(
                    &sum.into(),
                    Rvalue::BinaryOp(BinOp::Add, value, offset),
                    range,
                );
                Operand::Copy(sum.into())
            }
        }
    }

    /// Goes on if `place op value`, else to `fail`.
    fn compare(
        &mut self,
        op: BinOp,
        place: Place,
        value: Operand,
        fail: BasicBlock,
        range: TextRange,
    ) {
        let holds = self.temp(Ty::BOOL);
        self.assign(
            &holds.into(),
            Rvalue::BinaryOp(op, Operand::Copy(place), value),
            range,
        );
        let ok = self.new_block();
        self.terminate(
            TerminatorKind::SwitchInt {
                discr: Operand::Copy(holds.into()),
                targets: vec![(0, fail)],
                otherwise: ok,
                variants: None,
            },
            range,
        );
        self.current = Some(ok);
    }

    /// Goes on if the `&str` in `place` equals `value`, to `fail` if not.
    fn str_eq_test(
        &mut self,
        eq: flux_ty::Instance,
        place: Place,
        value: Operand,
        fail: BasicBlock,
        range: TextRange,
    ) {
        let holds = self.temp(Ty::BOOL);
        let args = vec![Operand::Copy(place), value];
        self.call(eq.function, eq.args, args, &holds.into(), range);
        let ok = self.new_block();
        self.terminate(
            TerminatorKind::SwitchInt {
                discr: Operand::Copy(holds.into()),
                targets: vec![(0, fail)],
                otherwise: ok,
                variants: None,
            },
            range,
        );
        self.current = Some(ok);
    }

    /// Binds `binding` to the value in `place`, or a reference to it, at
    /// the binding. A move out of what may not be moved out of is at
    /// `scrutinee`, where rustc reports it.
    fn bind(&mut self, binding: BindingId, place: Place, scrutinee: TextRange) {
        let local = self.binding_locals[binding];
        let mut range = self
            .binding_ranges
            .get(binding)
            .copied()
            .unwrap_or_default();
        let rvalue = match self.types.binding_refs.get(binding) {
            Some(&mutability) => Rvalue::Ref {
                mutability,
                place,
                two_phase: false,
            },
            None => {
                let operand = self.read(place.clone());
                if let Operand::Move(_) = operand
                    && (place.has_deref() || self.in_drop_owner(&place))
                {
                    range = scrutinee;
                }
                Rvalue::Use(operand)
            }
        };
        self.assign(&local.into(), rvalue, range);
    }

    fn bind_all(&mut self, binds: &[(BindingId, Place)], scrutinee: TextRange) {
        for (binding, place) in binds {
            self.bind(*binding, place.clone(), scrutinee);
        }
    }

    /// Whether `place` is inside a value whose type has a `Drop` impl.
    fn in_drop_owner(&self, place: &Place) -> bool {
        (0..place.projection.len()).any(|len| {
            let base = Place {
                local: place.local,
                projection: place.projection[..len].into(),
            };
            matches!(
                self.place_ty(&base).kind(),
                TyKind::Adt(adt, _)
                    if flux_ty::traits::drop_impl(self.items, &self.typed.sigs, adt.strukt).is_some()
            )
        })
    }

    /// Whether binding `binding` takes a value that is not `Copy`, which
    /// a guard must not move: it reads the place instead.
    fn binds_value(&self, binding: BindingId) -> bool {
        if self.types.binding_refs.get(binding).is_some() {
            return false;
        }
        let ty = self.locals[self.binding_locals[binding]].ty;
        !flux_ty::traits::is_copy(self.items, &self.typed.sigs, &self.env, ty)
    }

    /// `match scrutinee { arms }`: each arm's pattern is tested in turn,
    /// then its guard; the first that matches binds and runs its body.
    fn lower_match(
        &mut self,
        scrutinee: ExprId,
        arms: &[MatchArm],
        dest: &Place,
        range: TextRange,
    ) {
        let at = self.range(scrutinee);
        let place = self.pat_place(scrutinee);
        let join = self.new_block();
        for arm in arms {
            let next = self.new_block();
            self.scoped(ScopeKind::Block, range, |b| {
                b.declare_bindings(arm.pat);
                let exits = b.test(arm.pat, place.clone(), next, at);
                let body = b.new_block();
                for exit in exits {
                    b.current = Some(exit.block);
                    if let Some(guard) = arm.guard {
                        let (later, now): (Vec<_>, Vec<_>) = exit
                            .binds
                            .iter()
                            .cloned()
                            .partition(|(binding, _)| b.binds_value(*binding));
                        b.bind_all(&now, at);
                        for (binding, place) in &later {
                            b.guard_places.insert(*binding, place.clone());
                        }
                        let guard_range = b.range(guard);
                        let pass = b.new_block();
                        let failed = b.new_block();
                        let scrutinee = match &place {
                            PatPlace::Place(place) => Some(place.clone()),
                            PatPlace::Upvar(..) => None,
                        };
                        let outer = b.guard.replace(Guard {
                            scrutinee,
                            ref_muts: now
                                .iter()
                                .map(|&(binding, _)| binding)
                                .filter(|&binding| {
                                    b.types.binding_refs.get(binding) == Some(&Mutability::Mut)
                                })
                                .collect(),
                        });
                        if b.has_let(guard) {
                            b.lower_condition(guard, pass, failed);
                        } else {
                            let cond =
                                b.scoped(ScopeKind::Temps, guard_range, |b| b.lower_operand(guard));
                            b.terminate(
                                TerminatorKind::SwitchInt {
                                    discr: cond,
                                    targets: vec![(0, failed)],
                                    otherwise: pass,
                                    variants: None,
                                },
                                guard_range,
                            );
                        }
                        b.guard = outer;
                        for (binding, _) in &later {
                            b.guard_places.remove(*binding);
                        }
                        // What the guard saw is bound again if another
                        // way of matching is tried.
                        b.current = Some(failed);
                        let mut seen: Vec<BindingId> = now.iter().map(|&(b, _)| b).collect();
                        b.condition_bindings(guard, &mut seen);
                        for binding in &seen {
                            let Some(&local) = b.binding_locals.get(*binding) else {
                                continue;
                            };
                            let block = b.block();
                            b.blocks[block.index()].0.push(Statement {
                                kind: StatementKind::StorageDead(local),
                                range: guard_range,
                            });
                        }
                        b.goto(exit.resume, guard_range);
                        b.current = Some(pass);
                        b.bind_all(&later, at);
                    } else {
                        b.bind_all(&exit.binds, at);
                    }
                    b.goto(body, range);
                }
                b.current = Some(body);
                b.lower_into(arm.body, dest);
            });
            self.goto(join, range);
            self.current = Some(next);
        }
        // Exhaustive: no value gets here.
        self.terminate(TerminatorKind::Unreachable, range);
        self.current = Some(join);
    }

    /// The bindings of the `let`s of a condition.
    fn condition_bindings(&self, cond: ExprId, out: &mut Vec<BindingId>) {
        match self.body.exprs[cond] {
            Expr::Let { pat, .. } => self.body.pat_bindings(pat, out),
            Expr::Binary {
                op: BinaryOp::Logic(LogicOp::And),
                lhs,
                rhs,
            } => {
                self.condition_bindings(lhs, out);
                self.condition_bindings(rhs, out);
            }
            _ => {}
        }
    }

    /// Whether a condition is or has a `let`, in a chain of `&&`.
    fn has_let(&self, expr: ExprId) -> bool {
        match self.body.exprs[expr] {
            Expr::Let { .. } => true,
            Expr::Binary {
                op: BinaryOp::Logic(LogicOp::And),
                lhs,
                rhs,
            } => self.has_let(lhs) || self.has_let(rhs),
            _ => false,
        }
    }

    /// An `if` whose condition has `let`s. Their scrutinees'
    /// temporaries, and their bindings, live through the `then` branch,
    /// and are dropped before the `else` branch runs, as in edition
    /// 2024.
    fn lower_if_let(
        &mut self,
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
        dest: &Place,
        range: TextRange,
    ) {
        let depth = self.scopes.len();
        for kind in [ScopeKind::Temps, ScopeKind::Block] {
            self.scopes.push(Scope {
                kind,
                locals: Vec::new(),
            });
        }
        let then_block = self.new_block();
        let leave = self.new_block();
        let join = self.new_block();
        self.lower_condition(condition, then_block, leave);
        self.current = Some(then_block);
        self.lower_into(then_branch, dest);
        self.leave_scopes(depth, true, range);
        self.goto(join, range);
        self.current = Some(leave);
        self.leave_scopes(depth, true, range);
        self.scopes.truncate(depth);
        match else_branch {
            Some(else_branch) => self.lower_into(else_branch, dest),
            None => self.assign_unit(dest, range),
        }
        self.goto(join, range);
        self.current = Some(join);
    }

    /// Goes to `then` if `cond` holds, binding its `let`s' bindings,
    /// else to `otherwise`.
    fn lower_condition(&mut self, cond: ExprId, then: BasicBlock, otherwise: BasicBlock) {
        let range = self.range(cond);
        match self.body.exprs[cond] {
            Expr::Let { pat, expr } => {
                let at = self.range(expr);
                let place = self.pat_place(expr);
                self.declare_bindings(pat);
                for exit in self.test(pat, place, otherwise, at) {
                    self.current = Some(exit.block);
                    self.bind_all(&exit.binds, at);
                    self.goto(then, range);
                }
            }
            Expr::Binary {
                op: BinaryOp::Logic(LogicOp::And),
                lhs,
                rhs,
            } if self.has_let(cond) => {
                let middle = self.new_block();
                self.lower_condition(lhs, middle, otherwise);
                self.current = Some(middle);
                self.lower_condition(rhs, then, otherwise);
            }
            _ => {
                let value = self.lower_operand(cond);
                self.terminate(
                    TerminatorKind::SwitchInt {
                        discr: value,
                        targets: vec![(0, otherwise)],
                        otherwise: then,
                        variants: None,
                    },
                    range,
                );
            }
        }
    }

    /// `a && b` is `if a { b } else { false }`; `a || b` is
    /// `if a { true } else { b }`.
    fn lower_logic(
        &mut self,
        op: LogicOp,
        lhs: ExprId,
        rhs: ExprId,
        dest: &Place,
        range: TextRange,
    ) {
        let l = self.scoped(ScopeKind::Temps, range, |b| b.lower_operand(lhs));
        let rhs_block = self.new_block();
        let short_block = self.new_block();
        let join = self.new_block();
        let (on_false, on_true, short_value) = match op {
            LogicOp::And => (short_block, rhs_block, false),
            LogicOp::Or => (rhs_block, short_block, true),
        };
        self.terminate(
            TerminatorKind::SwitchInt {
                discr: l,
                targets: vec![(0, on_false)],
                otherwise: on_true,
                variants: None,
            },
            range,
        );
        self.current = Some(rhs_block);
        self.scoped(ScopeKind::Temps, range, |b| b.lower_into(rhs, dest));
        self.goto(join, range);
        self.current = Some(short_block);
        self.assign(
            dest,
            Rvalue::Use(Operand::Const(Constant::Bool(short_value))),
            range,
        );
        self.goto(join, range);
        self.current = Some(join);
    }

    /// `place = rhs` and `place op= rhs`. The right side is evaluated
    /// first, and the assignment itself carries the whole expression's
    /// range, where rustc reports assignment errors.
    fn lower_assign(&mut self, op: Option<ArithOp>, lhs: ExprId, rhs: ExprId, range: TextRange) {
        if !is_place(self.body, lhs) {
            self.bug("an assignment to a value", range);
            return;
        }
        self.check_guard_write(lhs, false, range);
        match op {
            None => {
                let value = self.lower_operand(rhs);
                let place = self.lower_place(lhs);
                if self.needs_drop(self.place_ty(&place)) {
                    // A whole binding's drop is its own, wherever it is.
                    let declared = self.scopes.iter().find_map(|scope| {
                        let found = scope.locals.iter().find(|&&(l, _)| l == place.local);
                        found
                            .filter(|_| place.projection.is_empty())
                            .map(|&(_, r)| r)
                    });
                    let range = declared.unwrap_or_else(|| self.range(lhs));
                    self.drop_place(place.clone(), range);
                }
                self.assign(&place, Rvalue::Use(value), range);
            }
            Some(arith) => {
                let r = self.lower_operand(rhs);
                let place = self.lower_place(lhs);
                let ty = self.place_ty(&place);
                let current = self.temp(ty);
                self.assign(
                    &current.into(),
                    Rvalue::Use(Operand::Copy(place.clone())),
                    range,
                );
                let rvalue = self.arith(arith, Operand::Copy(current.into()), r, ty, range);
                self.assign(&place, rvalue, range);
            }
        }
    }

    /// In a guard, what assigning or mutably borrowing (`borrow`) the
    /// place `expr` would break: the place matched (E0510), or the value
    /// behind a `ref mut` binding, which a guard sees as immutable.
    fn check_guard_write(&mut self, expr: ExprId, borrow: bool, range: TextRange) {
        let Some(guard) = &self.guard else {
            return;
        };
        if let Expr::Unary {
            op: UnaryOp::Deref,
            operand,
        } = self.body.exprs[expr]
            && let Expr::Path(Res::Local(binding)) = self.body.exprs[operand]
            && guard.ref_muts.contains(&binding)
        {
            let name = self.body.bindings[binding].name;
            let message = if borrow {
                format!(
                    "cannot borrow `*{name}` as mutable, as it is immutable for the pattern guard"
                )
            } else {
                format!("cannot assign to `*{name}`, as it is immutable for the pattern guard")
            };
            self.errors.push(flux_hir::Diagnostic::new(range, message));
            return;
        }
        let Some(written) = self.place_of(expr) else {
            return;
        };
        let Some(scrutinee) = &guard.scrutinee else {
            return;
        };
        let contains = written.local == scrutinee.local
            && scrutinee.projection.starts_with(&written.projection);
        if contains {
            let name = self.place_text(expr);
            let what = if borrow { "mutably borrow" } else { "assign" };
            self.errors.push(flux_hir::Diagnostic::new(
                range,
                format!("cannot {what} `{name}` in match guard"),
            ));
        }
    }

    /// The place a place expression denotes, without evaluating anything:
    /// a local and its fields, with no dereference.
    fn place_of(&self, expr: ExprId) -> Option<Place> {
        match self.body.exprs[expr] {
            Expr::Path(Res::Local(binding)) => Some((*self.binding_locals.get(binding)?).into()),
            Expr::Field { expr: base, .. } if self.types.field_derefs.get(expr).is_none() => {
                Some(self.place_of(base)?.field(self.field_index(expr)))
            }
            _ => None,
        }
    }

    /// A place expression as rustc writes it: `x.a`.
    fn place_text(&self, expr: ExprId) -> String {
        match self.body.exprs[expr] {
            Expr::Path(Res::Local(binding)) => self.body.bindings[binding].name.to_string(),
            Expr::Field { expr: base, name } => format!("{}.{name}", self.place_text(base)),
            Expr::Unary { operand, .. } => format!("*{}", self.place_text(operand)),
            Expr::Index { base, .. } => format!("{}[_]", self.place_text(base)),
            _ => "_".to_owned(),
        }
    }

    /// An arithmetic or bitwise operation, with the checks division needs.
    fn arith(&mut self, op: ArithOp, l: Operand, r: Operand, ty: Ty, range: TextRange) -> Rvalue {
        let op = match op {
            ArithOp::Add => BinOp::Add,
            ArithOp::Sub => BinOp::Sub,
            ArithOp::Mul => BinOp::Mul,
            ArithOp::Div => BinOp::Div,
            ArithOp::Rem => BinOp::Rem,
            ArithOp::Shl => BinOp::Shl,
            ArithOp::Shr => BinOp::Shr,
            ArithOp::BitXor => BinOp::BitXor,
            ArithOp::BitOr => BinOp::BitOr,
            ArithOp::BitAnd => BinOp::BitAnd,
        };
        if let (BinOp::Div | BinOp::Rem, Some(int)) = (op, ty.as_int()) {
            self.check_division(op, &l, &r, int, range);
        }
        Rvalue::BinaryOp(op, l, r)
    }

    /// Asserts that `r` is not zero and, for signed types, that `l / r` is
    /// not `MIN / -1`.
    fn check_division(
        &mut self,
        op: BinOp,
        l: &Operand,
        r: &Operand,
        int: IntTy,
        range: TextRange,
    ) {
        let (zero_msg, overflow_msg) = if op == BinOp::Div {
            (
                AssertMessage::DivisionByZero(l.clone()),
                AssertMessage::DivisionOverflow(l.clone(), r.clone()),
            )
        } else {
            (
                AssertMessage::RemainderByZero(l.clone()),
                AssertMessage::RemainderOverflow(l.clone(), r.clone()),
            )
        };
        // A constant divisor needs only the checks it could fail.
        let divisor = match r {
            Operand::Const(Constant::Int { bits, .. }) => Some(*bits),
            _ => None,
        };
        let minus_one = Constant::int(-1, int);
        if divisor.is_none() || divisor == Some(0) {
            let is_zero = self.temp(Ty::BOOL);
            let zero = Operand::Const(Constant::int(0, int));
            self.assign(
                &is_zero.into(),
                Rvalue::BinaryOp(BinOp::Eq, r.clone(), zero),
                range,
            );
            self.assert(Operand::Copy(is_zero.into()), false, zero_msg, range);
        }
        let may_be_minus_one = match (divisor, minus_one) {
            (Some(bits), Constant::Int { bits: m, .. }) => bits == m,
            _ => true,
        };
        if int.is_signed() && may_be_minus_one {
            let min = if int.bits() == 128 {
                i128::MIN
            } else {
                -(1i128 << (int.bits() - 1))
            };
            let is_minus_one = self.temp(Ty::BOOL);
            self.assign(
                &is_minus_one.into(),
                Rvalue::BinaryOp(BinOp::Eq, r.clone(), Operand::Const(minus_one)),
                range,
            );
            let is_min = self.temp(Ty::BOOL);
            let min = Operand::Const(Constant::int(min, int));
            self.assign(
                &is_min.into(),
                Rvalue::BinaryOp(BinOp::Eq, l.clone(), min),
                range,
            );
            let overflows = self.temp(Ty::BOOL);
            self.assign(
                &overflows.into(),
                Rvalue::BinaryOp(
                    BinOp::BitAnd,
                    Operand::Copy(is_minus_one.into()),
                    Operand::Copy(is_min.into()),
                ),
                range,
            );
            self.assert(Operand::Copy(overflows.into()), false, overflow_msg, range);
        }
    }

    fn assert(&mut self, cond: Operand, expected: bool, msg: AssertMessage, range: TextRange) {
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Assert {
                cond,
                expected,
                msg,
                target: next,
            },
            range,
        );
        self.current = Some(next);
    }
}

/// The bytes of a string or byte string literal, as the value of the
/// `str` or array they are: an aggregate of `u8`s.
fn text_value(literal: &Literal) -> Option<crate::eval::Value> {
    let bytes: &[u8] = match literal {
        Literal::Str(text) => text.as_bytes(),
        Literal::ByteStr(bytes) => bytes,
        _ => return None,
    };
    let byte = |b: &u8| crate::eval::Value::Int {
        bits: u128::from(*b),
        ty: IntTy::U8,
    };
    Some(crate::eval::Value::Aggregate(
        bytes.iter().map(byte).collect(),
    ))
}
