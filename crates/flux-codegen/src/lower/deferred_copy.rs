//! Keep an incoming aggregate in the caller's storage until an operation may change its bytes.

use flux_hir::Intrinsic;
use flux_mir::{
    AssertMessage, BasicBlock, Body, Local, Operand, Place, ProjectionElem, Rvalue, StatementKind,
    TerminatorKind,
};
use flux_ty::{Instance, Ty, TyKind};

use super::{Argument, COPY_EXPAND_BYTES, Ctx, Lower, MemoryStorage, forwarded_write_parameter};
use super::{is_memory, memory_storage};

/// The single copy-out use and the block where a potentially mutating call
/// needs the argument to have its own storage.
#[derive(Clone, Copy)]
pub(super) struct Plan {
    pub(super) argument: Local,
    pub(super) snapshot: BasicBlock,
    pub(super) write: BasicBlock,
}

const MAX_BODY_BLOCKS: usize = 24;
const MAX_QUERY_BLOCKS: usize = 12;
const MAX_QUERY_DEPTH: usize = 5;
const MAX_QUERY_BODIES: usize = 32;
const MIN_COPY_BYTES: u64 = 64;

pub(super) fn plan(lower: &mut Lower<'_, '_, '_>, storage: &MemoryStorage) -> Option<Plan> {
    let body = lower.body;
    if !lower.ctx.defer_aggregate_copies
        || lower.convention.is_c()
        || body.locals[Body::RETURN].ty != Ty::UNIT
        || !storage.acyclic
        || !(2..=MAX_BODY_BLOCKS).contains(&body.blocks.len())
        || body.has_unwind()
    {
        return None;
    }

    body.params().find_map(|argument| {
        let ty = body.locals[argument].ty;
        if !is_memory(lower.sigs, ty)
            || !matches!(
                lower.convention.argument(lower.sigs, &lower.ctx.layouts, ty),
                Argument::Direct
            )
            || !(MIN_COPY_BYTES..=COPY_EXPAND_BYTES).contains(&lower.ctx.layouts.of(ty).size)
        {
            return None;
        }
        let root = storage.roots[argument.index()];
        let is_argument = |place: &Place| storage.roots[place.local.index()] == root;
        if storage.fields.iter().any(|&(local, _, _)| storage.roots[local] == root)
            || body.params().any(|local| local != argument && storage.roots[local.index()] == root)
        {
            return None;
        }
        let consumed = sole_copy_out(body, storage, root)?;
        let TerminatorKind::Call { func, generic_args, args, dest, .. } =
            &body.blocks[consumed].terminator.kind
        else {
            return None;
        };
        if is_argument(dest)
            || body.place_ty(dest, lower.sigs) != Ty::UNIT
            || !matches!(body.operand_ty(&args[0], lower.sigs).kind(), TyKind::RawPtr(_, pointee) if pointee == ty)
            || !is_copy_out(lower.ctx, Instance { function: *func, args: generic_args })
        {
            return None;
        }
        let snapshot = first_mutation(lower.ctx, body, consumed)?;
        Some(Plan { argument, snapshot, write: consumed })
    })
}

/// The argument's local and its coalesced temporaries can only transfer
/// ownership within that slot and pass it once to the candidate writer.
fn sole_copy_out(body: &Body, storage: &MemoryStorage, root: usize) -> Option<BasicBlock> {
    let is_argument = |place: &Place| storage.roots[place.local.index()] == root;
    let is_read = |operand: &Operand| match operand {
        Operand::Move(place) | Operand::Copy(place) => is_argument(place),
        Operand::Const(_) => false,
    };
    let used = body
        .locals
        .iter()
        .filter(|&(local, _)| storage.roots[local.index()] == root)
        .map(|(local, _)| storage.uses[local.index()])
        .sum::<usize>();
    let mut accounted = 0;
    let mut write = None;

    for (block, data) in body.blocks.iter() {
        for statement in &data.statements {
            if let StatementKind::Assign(dest, value) = &statement.kind
                && is_argument(dest)
            {
                match value {
                    Rvalue::Use(Operand::Move(src) | Operand::Copy(src))
                        if dest.projection.is_empty()
                            && src.projection.is_empty()
                            && is_argument(src) =>
                    {
                        accounted += 1;
                    }
                    _ => return None,
                }
            }
        }
        match &data.terminator.kind {
            TerminatorKind::Call {
                args, dest, target, ..
            } => {
                if is_argument(dest) {
                    return None;
                }
                if let [_, Operand::Move(place) | Operand::Copy(place)] = args.as_slice()
                    && target.is_some()
                    && place.projection.is_empty()
                    && is_argument(place)
                {
                    if write.replace(block).is_some() || is_read(&args[0]) {
                        return None;
                    }
                    accounted += 1;
                }
            }
            TerminatorKind::CallIndirect { dest, .. } if is_argument(dest) => return None,
            // These operands describe a failed check and aren't part of
            // TerminatorKind::operands or the ordinary storage use counts.
            TerminatorKind::Assert { msg, .. } => {
                let reads_argument = match msg {
                    AssertMessage::DivisionByZero(value)
                    | AssertMessage::RemainderByZero(value) => is_read(value),
                    AssertMessage::DivisionOverflow(a, b)
                    | AssertMessage::RemainderOverflow(a, b) => is_read(a) || is_read(b),
                    AssertMessage::BoundsCheck { len, index } => is_read(len) || is_read(index),
                };
                if reads_argument {
                    return None;
                }
            }
            _ => {}
        }
    }
    (used == accounted).then_some(write).flatten()
}

fn read_only_statements(statements: &[flux_mir::Statement]) -> bool {
    statements.iter().all(|statement| match &statement.kind {
        StatementKind::Assign(dest, value) => {
            writes_local(dest)
                && match value {
                    Rvalue::InlineAsm { .. } => false,
                    Rvalue::WithAddr(..)
                    | Rvalue::OffsetOf(..)
                    | Rvalue::Use(..)
                    | Rvalue::BinaryOp(..)
                    | Rvalue::UnaryOp(..)
                    | Rvalue::Cast(..)
                    | Rvalue::Aggregate(..)
                    | Rvalue::Discriminant(..)
                    | Rvalue::DiscriminantForDrop(..)
                    | Rvalue::Ref { .. }
                    | Rvalue::Len { .. }
                    | Rvalue::Repeat(..)
                    | Rvalue::Unsize(..)
                    | Rvalue::Subslice { .. } => true,
                }
        }
        StatementKind::StorageDead(_) | StatementKind::PlaceMention(_) => true,
    })
}

fn writes_local(dest: &Place) -> bool {
    !dest
        .projection
        .iter()
        .any(|projection| matches!(projection, ProjectionElem::Deref | ProjectionElem::BoxDeref))
}

/// Follow the paths while the incoming bytes may still be used. One
/// unclassified call is allowed, and gets a copy immediately before it.
fn first_mutation(ctx: &mut Ctx<'_>, body: &Body, write: BasicBlock) -> Option<BasicBlock> {
    let mut visited = vec![false; body.blocks.len()];
    let mut pending = vec![Body::entry()];
    let mut snapshot = None;
    while let Some(block) = pending.pop() {
        if core::mem::replace(&mut visited[block.index()], true) {
            continue;
        }
        let data = &body.blocks[block];
        if !read_only_statements(&data.statements) {
            return None;
        }
        if block == write {
            continue;
        }
        match &data.terminator.kind {
            TerminatorKind::Call {
                func,
                generic_args,
                dest,
                ..
            } if writes_local(dest) => {
                let instance = Instance {
                    function: *func,
                    args: generic_args,
                };
                let mut budget = MAX_QUERY_BODIES;
                if !is_read_only(ctx, instance, 0, &mut budget) && snapshot.replace(block).is_some()
                {
                    return None;
                }
            }
            TerminatorKind::Goto(_)
            | TerminatorKind::SwitchInt { .. }
            | TerminatorKind::Return
            | TerminatorKind::Unreachable => {}
            _ => return None,
        }
        pending.extend(data.terminator.successors());
    }
    snapshot.filter(|_| visited[write.index()])
}

fn is_copy_out(ctx: &mut Ctx<'_>, instance: Instance) -> bool {
    let instance = ctx.resolve_instance(instance);
    let hir = ctx.hir;
    let info = &hir.items.functions[instance.function];
    if info.is_extern
        || info.link_name.is_some()
        || info.target_features.is_some()
        || info.intrinsic.is_some()
        || ctx.convention(instance).is_c()
    {
        return false;
    }
    let mir = ctx.mir;
    let Some((template, _)) = mir.instance_body(&hir.items, instance) else {
        return false;
    };
    if template.blocks.len() > MAX_QUERY_BLOCKS || template.param_count != 2 {
        return false;
    }
    let sigs = &ctx.typed.sigs;
    let body = flux_mir::mono::body_of(hir, sigs, mir, instance);
    let storage = memory_storage(&body, sigs);
    forwarded_write_parameter(&body, sigs, &storage.roots) == body.params().nth(1)
}

/// Classify resolved Rust bodies using a bounded walk. Reads through
/// references are harmless here; writes through pointers, opaque code and
/// calls without an available summary stop forwarding.
fn is_read_only(ctx: &mut Ctx<'_>, instance: Instance, depth: usize, budget: &mut usize) -> bool {
    if depth == MAX_QUERY_DEPTH || *budget == 0 {
        return false;
    }
    *budget -= 1;
    let instance = ctx.resolve_instance(instance);
    let hir = ctx.hir;
    let info = &hir.items.functions[instance.function];
    if info.is_extern || info.link_name.is_some() || info.target_features.is_some() {
        return false;
    }
    if let Some(intrinsic) = info.intrinsic {
        return matches!(
            intrinsic,
            Intrinsic::PtrAdd | Intrinsic::SizeOf | Intrinsic::AlignOf
        );
    }
    let mir = ctx.mir;
    let Some((template, _)) = mir.instance_body(&hir.items, instance) else {
        return false;
    };
    if template.blocks.len() > MAX_QUERY_BLOCKS {
        return false;
    }
    let body = flux_mir::mono::body_of(hir, &ctx.typed.sigs, mir, instance);
    !body.has_unwind()
        && body.blocks.values().all(|block| {
            read_only_statements(&block.statements)
                && match &block.terminator.kind {
                    TerminatorKind::Call {
                        func,
                        generic_args,
                        dest,
                        ..
                    } => {
                        writes_local(dest)
                            && is_read_only(
                                ctx,
                                Instance {
                                    function: *func,
                                    args: generic_args,
                                },
                                depth + 1,
                                budget,
                            )
                    }
                    TerminatorKind::Goto(_)
                    | TerminatorKind::SwitchInt { .. }
                    | TerminatorKind::Return
                    | TerminatorKind::Unreachable => true,
                    _ => false,
                }
        })
}
