//! Drop elaboration: deciding which drops run.
//!
//! [`build`](crate::build) drops every binding and temporary whose type
//! needs it where its scope ends, and every place before it is
//! overwritten, whether or not it still holds a value then. Here each
//! drop is matched against what may be initialized at it, as rustc's
//! `ElaborateDrops` does, with the [`init`](crate::init) analysis over
//! the dropped locals, where a drop moves out of its place:
//!
//! - A value moved out on every path is not dropped: the drop goes.
//! - One initialized on every path is dropped as it is.
//! - One moved out on some paths only gets a *drop flag*, a `bool`
//!   local that is set where it is assigned and cleared where it is
//!   moved, and is dropped if the flag is set.
//! - One with some fields moved out is dropped field by field, each as
//!   above; a struct with a `Drop` impl cannot be (E0509), so it is
//!   dropped whole or not at all. An enum with fields of a variant moved
//!   out drops the fields of the variant it holds, which it tests.
//!
//! What remains are drops of initialized values, which code generation
//! expands into calls of `Drop::drop` ([`mono::expand_drops`](crate::mono::expand_drops)).

use std::collections::{BTreeMap, HashSet};

use flux_hir::{Diagnostic, ItemTree};
use flux_ty::{Predicate, Signatures, Ty, TyKind};

use crate::init::{Event, MovePaths, State, analyze, statement_events, terminator_events};
use crate::{
    BasicBlock, BasicBlockData, Body, Constant, Local, LocalDecl, LocalKind, Operand, Place,
    ProjectionElem, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};

/// A drop to run: of `place`, if the flag of a move path says so, and if
/// each enum place holds its variant.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    place: Place,
    flag: Option<usize>,
    variants: Vec<(Place, u32)>,
}

/// Elaborates `body`'s drops; `env` is its elaborated bounds, for what
/// needs dropping. The drops that may run code are those of values
/// that may be held when dropped, or with `precise`, those elaboration
/// keeps (rustc's `const_precise_live_drops`).
pub(crate) fn elaborate(
    body: &mut Body,
    items: &ItemTree,
    sigs: &Signatures,
    env: &[Predicate],
    precise: bool,
) {
    let mut roots = HashSet::new();
    for data in body.blocks.values() {
        if let TerminatorKind::Drop { place, .. } = &data.terminator.kind
            && !place.has_deref()
        {
            roots.insert(place.local);
        }
    }
    if roots.is_empty() {
        // Only drops through pointers, which always find a value.
        body.live_drops = body
            .blocks
            .values()
            .filter_map(|data| match &data.terminator.kind {
                TerminatorKind::Drop { place, .. } => {
                    Some((data.terminator.range, body.place_ty(place, sigs)))
                }
                _ => None,
            })
            .collect();
        return;
    }
    let paths = MovePaths::new(body, |local| roots.contains(&local));
    let entry = analyze(body, &paths, true);
    let needs_drop = |ty| flux_ty::traits::needs_drop(items, sigs, env, ty);

    // What each drop becomes, and the flags that takes.
    let mut plans: Vec<(BasicBlock, usize, Vec<Step>)> = Vec::new();
    let mut flagged: BTreeMap<usize, Local> = BTreeMap::new();
    for (block, data) in body.blocks.iter() {
        let TerminatorKind::Drop { place, .. } = &data.terminator.kind else {
            continue;
        };
        let live = (data.terminator.range, body.place_ty(place, sigs));
        let Some((path, false)) = paths.find(place) else {
            body.live_drops.push(live);
            continue;
        };
        let mut steps = Vec::new();
        if let Some(mut state) = entry[block.index()].clone() {
            for statement in &data.statements {
                for event in statement_events(&statement.kind) {
                    paths.apply(&mut state, &event, true);
                }
            }
            let mut subtree = Vec::new();
            paths.subtree(path, &mut subtree);
            if !precise && subtree.iter().any(|&p| state.maybe_held[p]) {
                body.live_drops.push(live);
            }
            let ty = body.place_ty(place, sigs);
            let mut open = Open {
                paths: &paths,
                state: &state,
                items,
                sigs,
                needs_drop: &needs_drop,
                steps: &mut steps,
                variants: Vec::new(),
            };
            open.drop(path, place.clone(), ty);
            if precise && !steps.is_empty() {
                body.live_drops.push(live);
            }
        }
        for step in &steps {
            if let Some(flag) = step.flag {
                flagged.insert(flag, Local::from_raw(0));
            }
        }
        plans.push((block, path, steps));
    }
    for local in flagged.values_mut() {
        *local = body.locals.alloc(LocalDecl {
            ty: Ty::BOOL,
            kind: LocalKind::Temp,
        });
    }

    set_flags(body, &paths, &flagged);

    // Rewrite the drops.
    let mut dead = Vec::new();
    for (block, path, steps) in plans {
        let data = &body.blocks[block];
        let range = data.terminator.range;
        let TerminatorKind::Drop { place, target } = data.terminator.kind.clone() else {
            unreachable!("planned for drops")
        };
        if steps.is_empty() {
            body.blocks[block].terminator.kind = TerminatorKind::Goto(target);
            dead.push(block);
            continue;
        }
        let mut subtree = Vec::new();
        paths.subtree(path, &mut subtree);
        let resets: Vec<Statement> = subtree
            .iter()
            .filter_map(|p| flagged.get(p))
            .map(|&flag| set(flag, false, range))
            .collect();
        let whole = Step {
            place: place.clone(),
            flag: None,
            variants: Vec::new(),
        };
        if resets.is_empty() && steps.len() == 1 && steps[0] == whole {
            continue;
        }
        let mut next = target;
        if !resets.is_empty() {
            next = push(body, resets, TerminatorKind::Goto(target), range);
        }
        for (i, step) in steps.into_iter().enumerate().rev() {
            let mut kind = TerminatorKind::Drop {
                place: step.place,
                target: next,
            };
            let mut statements = Vec::new();
            if let Some(flag) = step.flag {
                let drop_block = push(body, Vec::new(), kind, range);
                kind = TerminatorKind::SwitchInt {
                    discr: Operand::Copy(flagged[&flag].into()),
                    targets: vec![(0, next)],
                    otherwise: drop_block,
                    variants: None,
                };
            }
            // The innermost enum's test last, the outermost's first.
            for (enum_place, variant) in step.variants.into_iter().rev() {
                let ty = body.place_ty(&enum_place, sigs);
                let TyKind::Adt(adt, _) = ty.kind() else {
                    continue;
                };
                let test_block = push(body, std::mem::take(&mut statements), kind, range);
                let discriminant = body.locals.alloc(LocalDecl {
                    ty: Ty::int(flux_ty::discriminant_ty(items, adt.strukt)),
                    kind: LocalKind::Temp,
                });
                statements.push(Statement {
                    kind: StatementKind::Assign(
                        discriminant.into(),
                        Rvalue::Discriminant(enum_place),
                    ),
                    range,
                });
                kind = TerminatorKind::SwitchInt {
                    discr: Operand::Copy(discriminant.into()),
                    targets: vec![(u128::from(variant), test_block)],
                    otherwise: next,
                    variants: Some(adt.strukt),
                };
            }
            if i == 0 {
                body.blocks[block].statements.extend(statements);
                body.blocks[block].terminator.kind = kind;
            } else {
                next = push(body, statements, kind, range);
            }
        }
    }

    // Each flag starts as its local does: set for a parameter.
    let inits = flagged.iter().map(|(&path, &flag)| {
        let param = body.is_param(paths.paths[path].local);
        set(flag, param, Default::default())
    });
    let inits: Vec<Statement> = inits.collect();
    let entry_block = &mut body.blocks[Body::entry()];
    entry_block.statements.splice(0..0, inits);

    merge_dead(body, &dead);
}

/// Decides how to drop a place from what may be initialized of it.
struct Open<'a> {
    paths: &'a MovePaths,
    state: &'a State,
    items: &'a ItemTree,
    sigs: &'a Signatures,
    needs_drop: &'a dyn Fn(Ty) -> bool,
    steps: &'a mut Vec<Step>,
    /// The enums, and their variants, the places dropped are inside of.
    variants: Vec<(Place, u32)>,
}

impl Open<'_> {
    fn step(&mut self, place: Place, flag: Option<usize>) {
        self.steps.push(Step {
            place,
            flag,
            variants: self.variants.clone(),
        });
    }

    /// Drops `place`, of type `ty`, whose move path is `path`.
    fn drop(&mut self, path: usize, place: Place, ty: Ty) {
        let mut subtree = Vec::new();
        self.paths.subtree(path, &mut subtree);
        if subtree.iter().all(|&p| !self.state.maybe_held[p]) {
            return;
        }
        if subtree.iter().all(|&p| !self.state.maybe_uninit[p]) {
            self.step(place, None);
            return;
        }
        let has_drop_impl = match ty.kind() {
            TyKind::Adt(adt, _) => {
                flux_ty::traits::drop_impl(self.items, self.sigs, adt.strukt).is_some()
            }
            _ => false,
        };
        let children = &self.paths.paths[path].children;
        if let (Some(variants), false, false) =
            (self.sigs.variants(ty), children.is_empty(), has_drop_impl)
        {
            // Each variant's fields, if it is the one the enum holds.
            for (v, fields) in variants.into_iter().enumerate() {
                let v = v as u32;
                let child = children.iter().copied().find(|&c| {
                    self.paths.paths[c].fields.last() == Some(&ProjectionElem::Downcast(v))
                });
                self.variants.push((place.clone(), v));
                self.fields(child.unwrap_or(path), path, &place.downcast(v), fields);
                self.variants.pop();
            }
            return;
        }
        let fields = self.sigs.fields(ty);
        let (Some(fields), false, false) = (fields, children.is_empty(), has_drop_impl) else {
            self.step(place, Some(path));
            return;
        };
        self.fields(path, path, &place, fields);
    }

    /// Drops the fields of `place`, whose types are `fields`: those with
    /// move paths below `path` by theirs, the others by `owner`'s own
    /// state, which only it and its ancestors change.
    fn fields(&mut self, path: usize, owner: usize, place: &Place, fields: &[Ty]) {
        let children = self.paths.paths[path].children.clone();
        for (i, &field_ty) in fields.iter().enumerate() {
            if !(self.needs_drop)(field_ty) {
                continue;
            }
            let field = place.field(i as u32);
            let child = children.iter().copied().find(|&c| {
                self.paths.paths[c].fields.last() == Some(&ProjectionElem::Field(i as u32))
            });
            match child {
                Some(child) => self.drop(child, field, field_ty),
                None if !self.state.maybe_held[owner] => {}
                None if !self.state.maybe_uninit[owner] => self.step(field, None),
                None => self.step(field, Some(owner)),
            }
        }
    }
}

fn set(flag: Local, value: bool, range: flux_syntax::TextRange) -> Statement {
    Statement {
        kind: StatementKind::Assign(
            flag.into(),
            Rvalue::Use(Operand::Const(Constant::Bool(value))),
        ),
        range,
    }
}

fn push(
    body: &mut Body,
    statements: Vec<Statement>,
    kind: TerminatorKind,
    range: flux_syntax::TextRange,
) -> BasicBlock {
    body.blocks.alloc(BasicBlockData {
        statements,
        terminator: Terminator { kind, range },
    })
}

/// Keeps each flag in step with its path: set where it, or a place
/// containing it, is assigned, and cleared where one is moved out of or
/// goes out of scope. Drops clear theirs where they are rewritten.
fn set_flags(body: &mut Body, paths: &MovePaths, flagged: &BTreeMap<usize, Local>) {
    if flagged.is_empty() {
        return;
    }
    // The flags an event sets or clears.
    let effects = |event: &Event| -> Vec<Statement> {
        let (path, value) = match *event {
            Event::Assign(place) => match paths.find(place) {
                Some((path, false)) => (path, true),
                _ => return Vec::new(),
            },
            Event::Move(place) => match paths.find(place) {
                Some((path, false)) => (path, false),
                _ => return Vec::new(),
            },
            Event::StorageDead(local) => match paths.find(&local.into()) {
                Some((path, _)) => (path, false),
                None => return Vec::new(),
            },
            Event::Read(_) | Event::Borrow(_) | Event::Drop(_) => return Vec::new(),
        };
        let mut subtree = Vec::new();
        paths.subtree(path, &mut subtree);
        subtree
            .iter()
            .filter_map(|p| flagged.get(p))
            .map(|&flag| set(flag, value, Default::default()))
            .collect()
    };
    let blocks: Vec<BasicBlock> = body.blocks.iter().map(|(b, _)| b).collect();
    for block in blocks {
        let data = &body.blocks[block];
        let mut statements = Vec::with_capacity(data.statements.len());
        for statement in &data.statements {
            statements.push(statement.clone());
            for event in statement_events(&statement.kind) {
                statements.extend(effects(&event).into_iter().map(|s| Statement {
                    range: statement.range,
                    ..s
                }));
            }
        }
        // A call moves its arguments when it starts and assigns its
        // destination when it returns, on the way to its target.
        let range = data.terminator.range;
        let mut on_return = Vec::new();
        for event in terminator_events(&data.terminator.kind) {
            let effects = effects(&event)
                .into_iter()
                .map(|s| Statement { range, ..s });
            match event {
                Event::Assign(_) => on_return.extend(effects),
                _ => statements.extend(effects),
            }
        }
        body.blocks[block].statements = statements;
        if on_return.is_empty() {
            continue;
        }
        if let TerminatorKind::Call {
            target: Some(target),
            ..
        }
        | TerminatorKind::CallIndirect {
            target: Some(target),
            ..
        } = body.blocks[block].terminator.kind
        {
            let returned = push(body, on_return, TerminatorKind::Goto(target), range);
            if let TerminatorKind::Call { target, .. }
            | TerminatorKind::CallIndirect { target, .. } =
                &mut body.blocks[block].terminator.kind
            {
                *target = Some(returned);
            }
        }
    }
}

/// Merges into each of `dead`, a block whose drop went, the block it
/// now goes to, which the drop split off from it.
fn merge_dead(body: &mut Body, dead: &[BasicBlock]) {
    if dead.is_empty() {
        return;
    }
    let mut preds = vec![0u32; body.blocks.len()];
    for data in body.blocks.values() {
        for successor in data.terminator.kind.successors() {
            preds[successor.index()] += 1;
        }
    }
    let dead_set: HashSet<BasicBlock> = dead.iter().copied().collect();
    let mut removed = vec![false; body.blocks.len()];
    for &block in dead {
        if removed[block.index()] {
            continue;
        }
        // A run of dead drops merges into its first.
        while let TerminatorKind::Goto(next) = body.blocks[block].terminator.kind {
            if next == block || next == Body::entry() || preds[next.index()] != 1 {
                break;
            }
            let merged = std::mem::replace(
                &mut body.blocks[next],
                BasicBlockData {
                    statements: Vec::new(),
                    terminator: Terminator {
                        kind: TerminatorKind::Unreachable,
                        range: Default::default(),
                    },
                },
            );
            removed[next.index()] = true;
            let data = &mut body.blocks[block];
            data.statements.extend(merged.statements);
            data.terminator = merged.terminator;
            // Where a call's `#[track_caller]` location points goes with
            // its terminator.
            match body.fn_spans.remove(&next) {
                Some(span) => body.fn_spans.insert(block, span),
                None => body.fn_spans.remove(&block),
            };
            if !dead_set.contains(&next) {
                break;
            }
        }
    }
    // Renumber the blocks that are left.
    let mut map = vec![BasicBlock::from_raw(0); removed.len()];
    let mut kept = flux_arena::Arena::new();
    let old = std::mem::take(&mut body.blocks);
    for (i, data) in old.into_values().enumerate() {
        if !removed[i] {
            map[i] = kept.alloc(data);
        }
    }
    for data in kept.values_mut() {
        retarget(&mut data.terminator.kind, &map);
    }
    body.blocks = kept;
    body.fn_spans = std::mem::take(&mut body.fn_spans)
        .into_iter()
        .filter(|(b, _)| !removed[b.index()])
        .map(|(b, span)| (map[b.index()], span))
        .collect();
}

fn retarget(kind: &mut TerminatorKind, map: &[BasicBlock]) {
    let new = |b: &mut BasicBlock| *b = map[b.index()];
    match kind {
        TerminatorKind::Goto(target)
        | TerminatorKind::Assert { target, .. }
        | TerminatorKind::Drop { target, .. } => new(target),
        TerminatorKind::SwitchInt {
            targets, otherwise, ..
        } => {
            targets.iter_mut().for_each(|(_, b)| new(b));
            new(otherwise);
        }
        TerminatorKind::Call { target, .. } | TerminatorKind::CallIndirect { target, .. } => {
            target.iter_mut().for_each(new)
        }
        TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

/// The drops in `body`, of a constant or `const fn`, that may run code:
/// `Drop` impls cannot run at compile time (E0493). Each is reported at
/// the binding or expression whose value it drops, once per place it is
/// dropped, as rustc does.
pub fn check_const(body: &Body) -> Vec<Diagnostic> {
    body.live_drops
        .iter()
        .map(|&(range, ty)| {
            Diagnostic::new(
                range,
                format!("destructor of `{ty}` cannot be evaluated at compile-time"),
            )
        })
        .collect()
}
