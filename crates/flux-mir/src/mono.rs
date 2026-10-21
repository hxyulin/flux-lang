//! Monomorphization: the instances of functions a program needs, each
//! a function with its generic arguments, and their bodies with those
//! arguments substituted.
//!
//! A generic function's MIR is built and checked once, in terms of its
//! parameters; code is generated once per instance. The instances are
//! found from the functions that are not generic, through their calls,
//! as rustc's collector does, so a generic function nothing calls has
//! none.

use std::collections::HashMap;

use flux_hir::{Diagnostic, FunctionId, Hir};
pub use flux_ty::Instance;
use flux_ty::{Signatures, Ty, TyKind, subst_list};

use crate::{Body, Constant, FileMir, Operand, Rvalue, StatementKind, TerminatorKind};

/// rustc's `recursion_limit`: how deep a chain of instances, each
/// found in the one before, may be.
const RECURSION_LIMIT: u32 = 128;

/// Every instance of a function with a body that the program needs, in
/// the order they were found; or an error if a generic function needs
/// instances of itself without end. A call of a trait's method is of
/// the `impl` block's method for its types.
pub fn collect(hir: &Hir, sigs: &Signatures, mir: &FileMir) -> Result<Vec<Instance>, Diagnostic> {
    let mut instances = Vec::new();
    let mut depths: HashMap<Instance, u32> = HashMap::new();
    for (function, _) in mir.bodies.iter() {
        if generic_count(hir, function) == 0 {
            let instance = Instance {
                function,
                args: &[],
            };
            depths.insert(instance, 0);
            instances.push(instance);
        }
    }
    let mut next = 0;
    while let Some(&caller) = instances.get(next) {
        next += 1;
        let depth = depths[&caller];
        let body = &mir.bodies[caller.function];
        for data in body.blocks.values() {
            let TerminatorKind::Call {
                func, generic_args, ..
            } = &data.terminator.kind
            else {
                continue;
            };
            let callee = Instance {
                function: *func,
                args: Ty::list(&subst_list(generic_args, caller.args)),
            };
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
    Ok(instances)
}

/// How many generic parameters `function` has, its `impl` block's
/// included.
fn generic_count(hir: &Hir, function: FunctionId) -> usize {
    let data = &hir.items.functions[function];
    data.generics.len() + flux_ty::traits::owner_param_count(&hir.items, data.owner)
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
pub fn subst_body(body: &Body, args: &[Ty]) -> Body {
    let mut body = body.clone();
    for local in body.locals.values_mut() {
        local.ty = local.ty.subst(args);
    }
    let operand = |op: &mut Operand| {
        if let Operand::Const(Constant::Item { ty, .. }) = op {
            *ty = ty.subst(args);
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
                    *ty = ty.subst(args);
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
                *generic_args = Ty::list(&subst_list(generic_args, args));
                call_args.iter_mut().for_each(operand);
            }
            TerminatorKind::SwitchInt { discr, .. } => operand(discr),
            TerminatorKind::Assert { cond, .. } => operand(cond),
            TerminatorKind::Goto(_) | TerminatorKind::Return | TerminatorKind::Unreachable => {}
        }
    }
    body
}
