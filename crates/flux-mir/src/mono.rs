//! Monomorphization: the instances of functions a program needs, each
//! a function with its generic arguments, and their bodies with those
//! arguments substituted.
//!
//! A generic function's MIR is built and checked once, in terms of its
//! parameters; code is generated once per instance. The instances are
//! found from the functions that are not generic, through their calls
//! and the `Drop` impls their drops run, as rustc's collector does, so a
//! generic function nothing calls has none.
//!
//! A drop of a value is expanded here, once its type is known, into its
//! *drop glue*: calls of `Drop::drop` for each part of it whose type has
//! a `Drop` impl, the outer before the inner and fields in order.

use std::borrow::Cow;
use std::collections::HashMap;

use flux_hir::{Diagnostic, FunctionId, Hir, ItemTree};
pub use flux_ty::Instance;
use flux_ty::{Mutability, Signatures, Ty, TyKind, subst_list};

use crate::{
    BasicBlockData, Body, Constant, FileMir, LocalDecl, LocalKind, Operand, Place, Rvalue,
    Statement, StatementKind, Terminator, TerminatorKind,
};

/// rustc's `recursion_limit`: how deep a chain of instances, each
/// found in the one before, may be.
const RECURSION_LIMIT: u32 = 128;

/// Every instance of a function with a body that the program needs, in
/// the order they were found; or an error if a generic function needs
/// instances of itself without end. A call of a trait's method is of
/// the `impl` block's method for its types. As rustc does for a program,
/// they are found from `main`: other functions nothing calls are not
/// built, nor are the instances only they need.
pub fn collect(hir: &Hir, sigs: &Signatures, mir: &FileMir) -> Result<Vec<Instance>, Diagnostic> {
    let mut instances = Vec::new();
    let mut depths: HashMap<Instance, u32> = HashMap::new();
    let items = &hir.items;
    let root = items.module_of(items.main_crate());
    let main = mir.bodies.iter().map(|(id, _)| id).find(|&id| {
        let f = &items.functions[id];
        f.name.as_str() == "main" && f.loc.module == root && f.owner.is_none()
    });
    if let Some(function) = main {
        let instance = Instance {
            function,
            args: &[],
        };
        depths.insert(instance, 0);
        instances.push(instance);
    }
    let mut next = 0;
    while let Some(&caller) = instances.get(next) {
        next += 1;
        let depth = depths[&caller];
        let body = &mir.bodies[caller.function];
        for data in body.blocks.values() {
            let callees = match &data.terminator.kind {
                TerminatorKind::Call {
                    func, generic_args, ..
                } => vec![Instance {
                    function: *func,
                    args: Ty::list(&subst_list(generic_args, caller.args)),
                }],
                TerminatorKind::Drop { place, .. } => {
                    let ty = body.place_ty(place, sigs);
                    let ty = flux_ty::traits::instance_ty(&hir.items, sigs, ty, caller.args);
                    let mut glue = Vec::new();
                    drop_glue(&hir.items, sigs, place.clone(), ty, &mut glue);
                    let drop = drop_fn(&hir.items);
                    glue.into_iter()
                        .filter_map(|(_, ty)| {
                            Some(Instance {
                                function: drop?,
                                args: Ty::list(&[ty]),
                            })
                        })
                        .collect()
                }
                _ => continue,
            };
            for callee in callees {
                let callee = flux_ty::traits::resolve_instance(&hir.items, sigs, callee);
                if mir.bodies.get(callee.function).is_none() {
                    continue;
                }
                if depths.contains_key(&callee) {
                    continue;
                }
                if depth + 1 > RECURSION_LIMIT {
                    let mut d = Diagnostic::new(
                        data.terminator.range,
                        format!(
                            "reached the recursion limit while instantiating `{}`",
                            instance_name(hir, callee)
                        ),
                    );
                    d.file = hir.items.functions[caller.function].loc.file;
                    return Err(d);
                }
                depths.insert(callee, depth + 1);
                instances.push(callee);
            }
        }
    }
    Ok(instances)
}

/// The MIR of `instance`, of a function whose MIR is `body`: with its
/// generic arguments substituted and its drops expanded.
pub fn instance_body<'b>(
    hir: &Hir,
    sigs: &Signatures,
    body: &'b Body,
    args: &[Ty],
) -> Cow<'b, Body> {
    let mut body = if args.is_empty() {
        Cow::Borrowed(body)
    } else {
        Cow::Owned(subst_body(hir, sigs, body, args))
    };
    let drops = body
        .blocks
        .values()
        .any(|data| matches!(data.terminator.kind, TerminatorKind::Drop { .. }));
    if drops {
        expand_drops(&hir.items, sigs, body.to_mut());
    }
    body
}

/// `Drop::drop`.
fn drop_fn(items: &ItemTree) -> Option<FunctionId> {
    let drop = flux_intern::sym::drop;
    let t = *items.lang_traits.get(&drop)?;
    items.traits[t].function(items, drop)
}

/// The parts of a `ty` at `place` whose types have a `Drop` impl, in the
/// order dropping it runs theirs: a struct's own, then its fields', in
/// order.
fn drop_glue(
    items: &ItemTree,
    sigs: &Signatures,
    place: Place,
    ty: Ty,
    out: &mut Vec<(Place, Ty)>,
) {
    match ty.kind() {
        TyKind::Adt(adt, _) => {
            if flux_ty::traits::drop_impl(items, sigs, adt.strukt).is_some() {
                out.push((place.clone(), ty));
            }
        }
        TyKind::Tuple(_) => {}
        _ => return,
    }
    for (i, &field) in sigs.fields(ty).unwrap_or_default().iter().enumerate() {
        drop_glue(items, sigs, place.field(i as u32), field, out);
    }
}

/// Replaces each drop in `body`, whose types are known, with its drop
/// glue: `Drop::drop(&mut place)` for each part that has a `Drop` impl.
pub fn expand_drops(items: &ItemTree, sigs: &Signatures, body: &mut Body) {
    let Some(drop) = drop_fn(items) else {
        return;
    };
    let unit = body.locals.alloc(LocalDecl {
        ty: Ty::UNIT,
        kind: LocalKind::Temp,
    });
    let blocks: Vec<_> = body.blocks.iter().map(|(b, _)| b).collect();
    for block in blocks {
        let range = body.blocks[block].terminator.range;
        let TerminatorKind::Drop { place, target } = body.blocks[block].terminator.kind.clone()
        else {
            continue;
        };
        let ty = body.place_ty(&place, sigs);
        let mut glue = Vec::new();
        drop_glue(items, sigs, place, ty, &mut glue);
        let mut next = target;
        let mut first = (Vec::new(), TerminatorKind::Goto(target));
        for (i, (place, ty)) in glue.into_iter().enumerate().rev() {
            let pointer = body.locals.alloc(LocalDecl {
                ty: Ty::new(TyKind::Ref(Mutability::Mut, ty)),
                kind: LocalKind::Temp,
            });
            let borrow = Statement {
                kind: StatementKind::Assign(
                    pointer.into(),
                    Rvalue::Ref {
                        mutability: Mutability::Mut,
                        place,
                        two_phase: false,
                    },
                ),
                range,
            };
            let call = TerminatorKind::Call {
                func: drop,
                generic_args: Ty::list(&[ty]),
                args: vec![Operand::Move(pointer.into())],
                dest: unit.into(),
                target: Some(next),
            };
            if i == 0 {
                first = (vec![borrow], call);
            } else {
                next = body.blocks.alloc(BasicBlockData {
                    statements: vec![borrow],
                    terminator: Terminator { kind: call, range },
                });
            }
        }
        let data = &mut body.blocks[block];
        data.statements.extend(first.0);
        data.terminator.kind = first.1;
    }
}

/// `f::<A, B>` as rustc writes an instance in messages, with types
/// nested more than nine deep written `_`.
fn instance_name(hir: &Hir, instance: Instance) -> String {
    let name = hir
        .items
        .function_path(&hir.items.functions[instance.function]);
    let args: Vec<String> = instance.args.iter().map(|&a| truncated(a, 9)).collect();
    format!("{name}::<{}>", args.join(", "))
}

fn truncated(ty: Ty, depth: u32) -> String {
    if depth == 0 {
        return "_".to_owned();
    }
    match ty.kind() {
        TyKind::Tuple(elements) if !elements.is_empty() => {
            let elements: Vec<String> = elements.iter().map(|&e| truncated(e, depth - 1)).collect();
            if elements.len() == 1 {
                format!("({},)", elements[0])
            } else {
                format!("({})", elements.join(", "))
            }
        }
        TyKind::Adt(adt, args) if !args.is_empty() => {
            let args: Vec<String> = args.iter().map(|&a| truncated(a, depth - 1)).collect();
            format!("{}<{}>", adt.name, args.join(", "))
        }
        _ => ty.to_string(),
    }
}

/// `body`, of a generic function, with generic arguments `args`
/// substituted for its parameters.
pub fn subst_body(hir: &Hir, sigs: &Signatures, body: &Body, args: &[Ty]) -> Body {
    let mut body = body.clone();
    let items = &hir.items;
    let subst = |ty: Ty| flux_ty::traits::instance_ty(items, sigs, ty, args);
    for local in body.locals.values_mut() {
        local.ty = subst(local.ty);
    }
    let operand = |op: &mut Operand| {
        if let Operand::Const(Constant::Item {
            ty,
            args: item_args,
            ..
        }) = op
        {
            *ty = subst(*ty);
            *item_args = Ty::list(&item_args.iter().map(|&a| subst(a)).collect::<Vec<_>>());
        }
    };
    for data in body.blocks.values_mut() {
        for statement in &mut data.statements {
            let StatementKind::Assign(_, rvalue) = &mut statement.kind else {
                continue;
            };
            match rvalue {
                Rvalue::Use(op) | Rvalue::UnaryOp(_, op) => operand(op),
                Rvalue::Cast(op, ty) => {
                    operand(op);
                    *ty = subst(*ty);
                }
                Rvalue::BinaryOp(_, l, r) => {
                    operand(l);
                    operand(r);
                }
                Rvalue::Aggregate(_, ops) => ops.iter_mut().for_each(operand),
                Rvalue::Ref { .. } => {}
            }
        }
        match &mut data.terminator.kind {
            TerminatorKind::Call {
                generic_args,
                args: call_args,
                ..
            } => {
                let list: Vec<Ty> = generic_args.iter().map(|&a| subst(a)).collect();
                *generic_args = Ty::list(&list);
                call_args.iter_mut().for_each(operand);
            }
            TerminatorKind::SwitchInt { discr, .. } => operand(discr),
            TerminatorKind::Assert { cond, .. } => operand(cond),
            TerminatorKind::Goto(_)
            | TerminatorKind::Drop { .. }
            | TerminatorKind::Return
            | TerminatorKind::Unreachable => {}
        }
    }
    body
}
