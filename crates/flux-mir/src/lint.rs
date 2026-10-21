//! rustc's deny-by-default lints for overflows and panics known at
//! compile time: `let x: u8 = 255; x + 1` is "this arithmetic operation
//! will overflow" (`arithmetic_overflow`), and `1 / 0` "this operation
//! will panic at runtime" (`unconditional_panic`). They apply even with
//! overflow checks off, so a program with them does not compile.
//!
//! As rustc's `KnownPanicsLint` does, this propagates known values
//! through the body's blocks in reverse postorder, once: a local assigned
//! once keeps its value everywhere, one assigned more often only within
//! the block that assigned it, and one that is borrowed never has a
//! known value.

use std::collections::HashMap;

use flux_arena::ArenaMap;
use flux_hir::Diagnostic;
use flux_syntax::TextRange;

use crate::eval::{Value, binary, sign_extend, truncate};
use crate::{
    BasicBlock, BinOp, Body, Constant, Local, Operand, Rvalue, StatementKind, TerminatorKind, UnOp,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Assigned once: its value holds wherever it is used.
    Everywhere,
    /// Assigned more than once: its value holds in the block that
    /// assigned it.
    InBlock,
    Never,
}

/// The lints' errors in `body`. `consts` are the constants' values.
pub fn check(
    body: &Body,
    sigs: &flux_ty::Signatures,
    consts: &ArenaMap<flux_hir::Const, Value>,
) -> Vec<Diagnostic> {
    if body.in_core {
        return Vec::new();
    }
    let modes = modes(body);
    let mut known: HashMap<Local, Value> = HashMap::new();
    let mut diagnostics = Vec::new();
    // Blocks a known condition rules out are not checked, as in rustc:
    // `if false { 255u8 + 1 }` compiles.
    let mut reachable = vec![false; body.blocks.len()];
    reachable[0] = true;
    for block in reverse_postorder(body) {
        if !reachable[block.index()] {
            continue;
        }
        let data = &body.blocks[block];
        let mut assigned_here = Vec::new();
        for statement in &data.statements {
            let StatementKind::Assign(dest, rvalue) = &statement.kind else {
                continue;
            };
            let value = evaluate(
                body,
                sigs,
                rvalue,
                &known,
                consts,
                statement.range,
                &mut diagnostics,
            );
            if !dest.projection.is_empty() {
                continue;
            }
            match (value, modes[dest.local.index()]) {
                (Some(value), Mode::Everywhere | Mode::InBlock) => {
                    known.insert(dest.local, value);
                    assigned_here.push(dest.local);
                }
                _ => {
                    known.remove(&dest.local);
                }
            }
        }
        let taken = match &data.terminator.kind {
            TerminatorKind::SwitchInt {
                discr,
                targets,
                otherwise,
            } => match operand(discr, &known, consts) {
                Some(Value::Int { bits, .. }) => Some(
                    targets
                        .iter()
                        .find(|&&(v, _)| v == bits)
                        .map_or(*otherwise, |&(_, t)| t),
                ),
                Some(Value::Bool(b)) => Some(
                    targets
                        .iter()
                        .find(|&&(v, _)| v == u128::from(b))
                        .map_or(*otherwise, |&(_, t)| t),
                ),
                _ => None,
            },
            _ => None,
        };
        match taken {
            Some(target) => reachable[target.index()] = true,
            None => {
                for successor in data.terminator.kind.successors() {
                    reachable[successor.index()] = true;
                }
            }
        }
        match &data.terminator.kind {
            TerminatorKind::Assert { cond, expected, .. } => {
                if let Some(Value::Bool(value)) = operand(cond, &known, consts)
                    && value != *expected
                {
                    diagnostics.push(Diagnostic::new(
                        data.terminator.range,
                        "this operation will panic at runtime",
                    ));
                }
            }
            TerminatorKind::Call { dest, .. } => {
                known.remove(&dest.local);
            }
            _ => {}
        }
        for local in assigned_here {
            if modes[local.index()] == Mode::InBlock {
                known.remove(&local);
            }
        }
    }
    diagnostics
}

/// How far each local's known value may be propagated.
fn modes(body: &Body) -> Vec<Mode> {
    let mut assignments = vec![0u32; body.locals.len()];
    let mut modes = vec![Mode::Everywhere; body.locals.len()];
    for data in body.blocks.values() {
        for statement in &data.statements {
            if let StatementKind::Assign(dest, rvalue) = &statement.kind {
                assignments[dest.local.index()] += 1;
                if !dest.projection.is_empty() {
                    modes[dest.local.index()] = Mode::Never;
                }
                if let Rvalue::Ref { place, .. } = rvalue
                    && !place.has_deref()
                {
                    modes[place.local.index()] = Mode::Never;
                }
            }
        }
        if let TerminatorKind::Call { dest, .. } = &data.terminator.kind {
            assignments[dest.local.index()] += 1;
        }
    }
    for (mode, &count) in modes.iter_mut().zip(&assignments) {
        if *mode == Mode::Everywhere && count > 1 {
            *mode = Mode::InBlock;
        }
    }
    modes
}

fn reverse_postorder(body: &Body) -> Vec<BasicBlock> {
    let mut visited = vec![false; body.blocks.len()];
    let mut order = Vec::new();
    // Iterative depth-first search; a block is finished after its
    // successors.
    let mut stack = vec![(Body::entry(), 0usize)];
    visited[0] = true;
    while let Some((block, next)) = stack.pop() {
        let successors = body.blocks[block].terminator.kind.successors();
        if let Some(&successor) = successors.get(next) {
            stack.push((block, next + 1));
            if !visited[successor.index()] {
                visited[successor.index()] = true;
                stack.push((successor, 0));
            }
        } else {
            order.push(block);
        }
    }
    order.reverse();
    order
}

fn operand(
    op: &Operand,
    known: &HashMap<Local, Value>,
    consts: &ArenaMap<flux_hir::Const, Value>,
) -> Option<Value> {
    match op {
        Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
            known.get(&place.local).cloned()
        }
        Operand::Copy(_) | Operand::Move(_) => None,
        Operand::Const(Constant::Int { bits, ty }) => Some(Value::Int {
            bits: *bits,
            ty: *ty,
        }),
        Operand::Const(Constant::Bool(b)) => Some(Value::Bool(*b)),
        Operand::Const(Constant::Item { id, .. }) => match consts.get(*id)? {
            value @ (Value::Int { .. } | Value::Bool(_)) => Some(value.clone()),
            Value::Aggregate(_) => None,
        },
        Operand::Const(Constant::Unit | Constant::CStr(_)) => None,
    }
}

/// The value of `rvalue`, if known, reporting an operation known to
/// overflow.
fn evaluate(
    body: &Body,
    sigs: &flux_ty::Signatures,
    rvalue: &Rvalue,
    known: &HashMap<Local, Value>,
    consts: &ArenaMap<flux_hir::Const, Value>,
    range: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Value> {
    let overflow = |diagnostics: &mut Vec<Diagnostic>| {
        diagnostics.push(Diagnostic::new(
            range,
            "this arithmetic operation will overflow",
        ));
    };
    match rvalue {
        Rvalue::Use(op) => operand(op, known, consts),
        Rvalue::BinaryOp(op @ (BinOp::Shl | BinOp::Shr), l, r) => {
            // A known shift amount is enough to know it overflows.
            let amount = operand(r, known, consts)?;
            let width = body.operand_ty(l, sigs).as_int()?.bits();
            if let Value::Int { bits, ty } = amount
                && (sign_extend(bits, ty) < 0 || sign_extend(bits, ty) >= i128::from(width))
            {
                overflow(diagnostics);
                return None;
            }
            binary(*op, &operand(l, known, consts)?, &amount).ok()
        }
        Rvalue::BinaryOp(op, l, r) => {
            let (l, r) = (operand(l, known, consts)?, operand(r, known, consts)?);
            match binary(*op, &l, &r) {
                Ok(value) => Some(value),
                // Division is checked by an assertion before it.
                Err(_) if matches!(op, BinOp::Div | BinOp::Rem) => None,
                Err(_) => {
                    overflow(diagnostics);
                    None
                }
            }
        }
        Rvalue::UnaryOp(op, value) => match (op, operand(value, known, consts)?) {
            (UnOp::Not, Value::Bool(b)) => Some(Value::Bool(!b)),
            (UnOp::Not, Value::Int { bits, ty }) => Some(Value::Int {
                bits: truncate(!bits, ty),
                ty,
            }),
            (UnOp::Neg, Value::Int { bits, ty }) => {
                if ty.is_signed() && bits == 1u128 << (ty.bits() - 1) {
                    overflow(diagnostics);
                    return None;
                }
                Some(Value::Int {
                    bits: truncate(bits.wrapping_neg(), ty),
                    ty,
                })
            }
            _ => None,
        },
        Rvalue::Cast(op, to) => {
            let bits = match operand(op, known, consts)? {
                Value::Int { bits, ty } => sign_extend(bits, ty) as u128,
                Value::Bool(b) => u128::from(b),
                Value::Aggregate(_) => return None,
            };
            let to = to.as_int()?;
            Some(Value::Int {
                bits: truncate(bits, to),
                ty: to,
            })
        }
        Rvalue::Aggregate(..) | Rvalue::Ref { .. } => None,
    }
}
