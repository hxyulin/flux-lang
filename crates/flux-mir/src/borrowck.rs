//! Borrow checking: a borrow must not conflict with other uses of what it
//! borrows while the reference may still be used.
//!
//! Each `&place` or `&mut place` is a *loan*. A reference lives in a local
//! (a binding or a temporary), and copying it, or reborrowing through it
//! (`&mut *r`), makes another local *hold* the loan too. A loan is *live*
//! wherever a local holding it is live, that is, may still be read
//! later. This is rustc's non-lexical lifetimes for the subset flux
//! accepts, where a reference is never returned, stored in another type,
//! or assigned to a local more than once: then a loan's region is exactly
//! the points where its holders are live.
//!
//! While a loan is live, its place may not be moved out of (E0505) or
//! assigned (E0506); a mutable loan also forbids reading it (E0503) and
//! borrowing it again (E0499, E0502), and a shared one forbids borrowing
//! it mutably (E0502). Places conflict when one contains the other:
//! `s.a` and `s.b` do not. A *two-phase* loan, a method receiver's
//! auto-borrow, acts as a shared loan until the call activates it. A
//! binding whose scope ends while borrowed does not live long enough
//! (E0597).
//!
//! This pass also checks what may be written or borrowed mutably: a
//! binding declared without `mut` (E0596, E0594, the latter for fields;
//! assignments to whole locals are [`init`](crate::init)'s) and anything
//! behind a shared reference or `*const` pointer, and that nothing is
//! moved out from behind a pointer (E0507).

use flux_hir::{Diagnostic, ItemTree};
use flux_syntax::TextRange;
use flux_ty::{Mutability, Signatures, Ty, TyKind};

use crate::bitset::BitSet;
use crate::{
    BasicBlock, Body, Local, LocalKind, Operand, Place, ProjectionElem, Rvalue, StatementKind,
    TerminatorKind, place_name,
};

/// The diagnostics for `body`'s conflicting borrows and its writes and
/// moves through pointers.
pub fn check(body: &Body, items: &ItemTree, sigs: &Signatures) -> Vec<Diagnostic> {
    let loans = collect_loans(body);
    let holds = holders(body, &loans);
    // The locals holding each loan, few, for finding the live loans.
    let mut held_by: Vec<Vec<Local>> = vec![Vec::new(); loans.len()];
    for (local, held) in holds.iter().enumerate() {
        for &id in held {
            held_by[id as usize].push(Local::from_raw(local as u32));
        }
    }
    // The loans of each local's places, and those made into each local.
    let mut borrowed = vec![Vec::new(); body.locals.len()];
    let mut made_into = vec![Vec::new(); body.locals.len()];
    for (id, (loan, holder)) in loans.iter().enumerate() {
        borrowed[loan.place.local.index()].push(id as u32);
        made_into[holder.index()].push(id as u32);
    }
    let live_out = liveness(body, &holds);
    let mut checker = Checker {
        body,
        items,
        sigs,
        loans: &loans,
        holds: &holds,
        held_by: &held_by,
        borrowed: &borrowed,
        made_into: &made_into,
        diagnostics: Vec::new(),
        reported_dead: vec![false; loans.len()],
        in_aggregate: false,
    };
    let mut live = BitSet::new_empty(body.locals.len());
    // What the terminator and each statement, from the last, change in
    // `live` walking backwards: each local set and whether it was live,
    // and where each one's changes end.
    let mut changes: Vec<(Local, bool)> = Vec::new();
    let mut ends: Vec<usize> = Vec::new();
    for (block, data) in body.blocks.iter() {
        // From the locals live at the block's end, walking backwards to
        // its start; only those holding loans matter.
        for &local in &live_out[block.index()] {
            live.insert(local as usize);
        }
        changes.clear();
        ends.clear();
        let record = |changes: &mut Vec<_>, live: &mut BitSet, local: Local, value| {
            if !holds[local.index()].is_empty() {
                changes.push((local, live.set(local.index(), value)));
            }
        };
        terminator_transfer(&data.terminator.kind, &mut |local, value| {
            record(&mut changes, &mut live, local, value)
        });
        ends.push(changes.len());
        for statement in data.statements.iter().rev() {
            statement_transfer(&statement.kind, &mut |local, value| {
                record(&mut changes, &mut live, local, value)
            });
            ends.push(changes.len());
        }
        // Then forwards, undoing each statement's changes for the locals
        // live after it.
        let count = data.statements.len();
        for (i, statement) in data.statements.iter().enumerate() {
            let from_end = count - 1 - i;
            for &(local, was) in changes[ends[from_end]..ends[from_end + 1]].iter().rev() {
                live.set(local.index(), was);
            }
            checker.statement(block, i, &statement.kind, statement.range, &live);
        }
        for &(local, was) in changes[..ends[0]].iter().rev() {
            live.set(local.index(), was);
        }
        checker.terminator(&data.terminator.kind, data.terminator.range, &live);
        for &local in &live_out[block.index()] {
            live.remove(local as usize);
        }
    }
    checker.diagnostics
}

struct Loan {
    place: Place,
    mutability: Mutability,
    two_phase: bool,
    /// Where it was made.
    block: BasicBlock,
    statement: usize,
    range: TextRange,
}

fn collect_loans(body: &Body) -> Vec<(Loan, Local)> {
    let mut loans = Vec::new();
    for (block, data) in body.blocks.iter() {
        for (i, statement) in data.statements.iter().enumerate() {
            if let StatementKind::Assign(
                dest,
                Rvalue::Ref {
                    mutability,
                    place,
                    two_phase,
                },
            ) = &statement.kind
            {
                let loan = Loan {
                    place: place.clone(),
                    mutability: *mutability,
                    two_phase: *two_phase,
                    block,
                    statement: i,
                    range: statement.range,
                };
                loans.push((loan, dest.local));
            }
        }
    }
    loans
}

/// For each local, the loans it may hold, in order: those made into it,
/// those of references copied into it, and those of the reference a
/// reborrow goes through.
fn holders(body: &Body, loans: &[(Loan, Local)]) -> Vec<Vec<u32>> {
    let mut holds = vec![Vec::new(); body.locals.len()];
    for (id, (_, holder)) in loans.iter().enumerate() {
        holds[holder.index()].push(id as u32);
    }
    // `to` holds what `from` holds.
    let mut flows: Vec<(Local, Local)> = Vec::new();
    for (loan, holder) in loans {
        if loan.place.has_deref() {
            flows.push((loan.place.local, *holder));
        }
    }
    for data in body.blocks.values() {
        for statement in &data.statements {
            if let StatementKind::Assign(dest, Rvalue::Use(Operand::Copy(src) | Operand::Move(src))) =
                &statement.kind
                && src.projection.is_empty()
            {
                flows.push((src.local, dest.local));
            }
        }
    }
    // Until nothing changes, from each local whose loans grew.
    flows.sort_unstable();
    flows.dedup();
    let mut queued = BitSet::new_empty(body.locals.len());
    let mut worklist: Vec<Local> = Vec::new();
    for (local, held) in holds.iter().enumerate() {
        if !held.is_empty() {
            queued.insert(local);
            worklist.push(Local::from_raw(local as u32));
        }
    }
    let mut merged = Vec::new();
    while let Some(from) = worklist.pop() {
        queued.remove(from.index());
        let start = flows.partition_point(|&(f, _)| f < from);
        for &(_, to) in flows[start..].iter().take_while(|&&(f, _)| f == from) {
            if to == from {
                continue;
            }
            let (held, has) = (&holds[from.index()], &holds[to.index()]);
            merged.clear();
            let (mut i, mut j) = (0, 0);
            while i < held.len() || j < has.len() {
                let next = match (held.get(i), has.get(j)) {
                    (Some(&a), Some(&b)) if a == b => {
                        (i, j) = (i + 1, j + 1);
                        a
                    }
                    (Some(&a), Some(&b)) if a < b => {
                        i += 1;
                        a
                    }
                    (Some(&a), None) => {
                        i += 1;
                        a
                    }
                    (_, Some(&b)) => {
                        j += 1;
                        b
                    }
                    (None, None) => unreachable!("one is left"),
                };
                merged.push(next);
            }
            if merged.len() > has.len() {
                std::mem::swap(&mut holds[to.index()], &mut merged);
                if !queued.contains(to.index()) {
                    queued.insert(to.index());
                    worklist.push(to);
                }
            }
        }
    }
    holds
}

/// The locals a statement reads, as values or as pointers to go through.
fn place_uses(place: &Place, uses: &mut Vec<Local>) {
    if place.has_deref() {
        uses.push(place.local);
    }
}

fn operand_uses(operand: &Operand, uses: &mut Vec<Local>) {
    if let Operand::Copy(place) | Operand::Move(place) = operand {
        uses.push(place.local);
    }
}

/// Updates the live locals from after the statement to before it,
/// through `set`.
fn statement_transfer(kind: &StatementKind, set: &mut dyn FnMut(Local, bool)) {
    let mut uses = Vec::new();
    match kind {
        StatementKind::Assign(dest, rvalue) => {
            if dest.projection.is_empty() {
                set(dest.local, false);
            }
            place_uses(dest, &mut uses);
            match rvalue {
                Rvalue::Ref { place, .. } => place_uses(place, &mut uses),
                _ => {
                    for operand in rvalue.operands() {
                        operand_uses(operand, &mut uses);
                    }
                }
            }
        }
        StatementKind::StorageDead(local) => set(*local, false),
        StatementKind::PlaceMention(place) => uses.push(place.local),
    }
    for local in uses {
        set(local, true);
    }
}

fn terminator_transfer(kind: &TerminatorKind, set: &mut dyn FnMut(Local, bool)) {
    if let TerminatorKind::Call { dest, .. } | TerminatorKind::CallIndirect { dest, .. } = kind
        && dest.projection.is_empty()
    {
        set(dest.local, false);
    }
    let mut uses = Vec::new();
    for operand in kind.operands() {
        operand_uses(operand, &mut uses);
    }
    match kind {
        TerminatorKind::Call { dest, .. } | TerminatorKind::CallIndirect { dest, .. } => {
            place_uses(dest, &mut uses)
        }
        // Dropping a value may use the references in it.
        TerminatorKind::Drop { place, .. } => uses.push(place.local),
        _ => {}
    }
    for local in uses {
        set(local, true);
    }
}

/// The locals live at the end of each block, in order, of those that
/// `holds` says hold loans: whether one is live depends on its own uses
/// alone, and only those holding loans matter. A block's are few, so
/// they are listed rather than a bit each for every local.
fn liveness(body: &Body, holds: &[Vec<u32>]) -> Vec<Vec<u32>> {
    let mut live_in = vec![Vec::new(); body.blocks.len()];
    let mut live_out = vec![Vec::new(); body.blocks.len()];
    // The locals live at a point, and a list of them that may also have
    // those since found dead.
    let mut live = BitSet::new_empty(body.locals.len());
    let mut members: Vec<u32> = Vec::new();
    let blocks: Vec<_> = body.blocks.iter().collect();
    let mut changed = true;
    while changed {
        changed = false;
        for &(block, data) in blocks.iter().rev() {
            members.clear();
            for successor in data.terminator.kind.successors() {
                for &local in &live_in[successor.index()] {
                    if !live.set(local as usize, true) {
                        members.push(local);
                    }
                }
            }
            members.sort_unstable();
            live_out[block.index()].clone_from(&members);
            let mut set = |local: Local, value: bool| {
                if !holds[local.index()].is_empty() && !live.set(local.index(), value) && value {
                    members.push(local.index() as u32);
                }
            };
            terminator_transfer(&data.terminator.kind, &mut set);
            for statement in data.statements.iter().rev() {
                statement_transfer(&statement.kind, &mut set);
            }
            members.retain(|&local| live.contains(local as usize));
            members.sort_unstable();
            members.dedup();
            for &local in &members {
                live.remove(local as usize);
            }
            if members != live_in[block.index()] {
                live_in[block.index()].clone_from(&members);
                changed = true;
            }
        }
    }
    live_out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    Read,
    Move,
    Write,
    Borrow(Mutability),
    /// A two-phase `&mut` before its call: it conflicts as a shared
    /// borrow does, but is reported as the mutable borrow it is.
    Reserve,
}

struct Checker<'a> {
    body: &'a Body,
    items: &'a ItemTree,
    sigs: &'a Signatures,
    loans: &'a [(Loan, Local)],
    /// The loans each local holds, in order.
    holds: &'a [Vec<u32>],
    held_by: &'a [Vec<Local>],
    /// The loans of each local's places, in order: those an access to a
    /// place of it may conflict with.
    borrowed: &'a [Vec<u32>],
    /// The loans made into each local, in order.
    made_into: &'a [Vec<u32>],
    diagnostics: Vec<Diagnostic>,
    /// Loans already reported for outliving what they borrow.
    reported_dead: Vec<bool>,
    /// Checking the operands of an aggregate.
    in_aggregate: bool,
}

/// The loans live at a point: those a live local holds and, during a
/// call, those its arguments hold, except `except`.
#[derive(Clone, Copy)]
struct Live<'b> {
    locals: &'b BitSet,
    except: Option<usize>,
    args: &'b [Local],
}

/// Whether one place contains the other: they are the same local and
/// one projection is a prefix of the other. Elements of an array may be
/// the same whatever their indices, as rustc assumes.
fn overlaps(a: &Place, b: &Place) -> bool {
    a.local == b.local
        && a.projection
            .iter()
            .zip(b.projection.iter())
            .all(|(x, y)| match (x, y) {
                (ProjectionElem::Index(_), ProjectionElem::Index(_)) => true,
                _ => x == y,
            })
}

impl Checker<'_> {
    fn error(&mut self, range: TextRange, message: String) {
        // One error per place in the source: `x += 1` both reads and
        // writes `x`, and rustc reports only the first conflict.
        if self.diagnostics.iter().any(|d| d.range == range) {
            return;
        }
        self.diagnostics.push(Diagnostic::new(range, message));
    }

    fn name(&self, place: &Place) -> String {
        place_name(self.body, self.items, self.sigs, place)
    }

    /// If `place` is a static or part of one, not behind another
    /// pointer: the static's name, whether it is mutable, and whether
    /// `place` is all of it.
    fn static_of(&self, place: &Place) -> Option<(String, bool, bool)> {
        let LocalKind::Static(id) = self.body.locals[place.local].kind else {
            return None;
        };
        if place
            .projection
            .iter()
            .rposition(|&e| e == ProjectionElem::Deref)
            != Some(0)
        {
            return None;
        }
        let data = &self.items.consts[id];
        let name = data.name.map_or("_".to_owned(), |n| n.to_string());
        Some((name, data.is_static_mut(), place.projection.len() == 1))
    }

    /// Whether loan `id` is `live`.
    fn is_live(&self, id: usize, live: &Live) -> bool {
        Some(id) != live.except
            && (self.held_by[id]
                .iter()
                .any(|h| live.locals.contains(h.index()))
                || live
                    .args
                    .iter()
                    .any(|arg| self.holds[arg.index()].binary_search(&(id as u32)).is_ok()))
    }

    fn statement(
        &mut self,
        block: BasicBlock,
        index: usize,
        kind: &StatementKind,
        range: TextRange,
        live_after: &BitSet,
    ) {
        match kind {
            StatementKind::Assign(dest, rvalue) => {
                let own = self
                    .loans
                    .binary_search_by_key(&(block, index), |(l, _)| (l.block, l.statement))
                    .ok();
                let live = &Live {
                    locals: live_after,
                    except: own,
                    args: &[],
                };
                match rvalue {
                    Rvalue::Ref {
                        mutability,
                        place,
                        two_phase,
                    } => {
                        if *mutability == Mutability::Mut {
                            self.check_mutable_borrow(place, range);
                        }
                        // A two-phase borrow is shared until the call.
                        let access = if *two_phase {
                            Access::Reserve
                        } else {
                            Access::Borrow(*mutability)
                        };
                        self.access(place, access, live, range);
                    }
                    // `len` borrows its receiver; a bounds check reads
                    // only the pointer a slice is behind.
                    Rvalue::Len {
                        place,
                        borrows: true,
                    } => self.access(place, Access::Borrow(Mutability::Not), live, range),
                    Rvalue::Len { .. } => {}
                    _ => {
                        self.in_aggregate = matches!(rvalue, Rvalue::Aggregate(..));
                        for operand in rvalue.operands() {
                            self.operand(operand, live, range);
                        }
                    }
                }
                self.write(dest, live, range);
            }
            StatementKind::PlaceMention(_) => {}
            StatementKind::StorageDead(local) => {
                let live = &Live {
                    locals: live_after,
                    except: None,
                    args: &[],
                };
                let borrowed = self.borrowed;
                for &id in &borrowed[local.index()] {
                    let id = id as usize;
                    let loan = &self.loans[id].0;
                    if self.is_live(id, live) && !loan.place.has_deref() && !self.reported_dead[id]
                    {
                        self.reported_dead[id] = true;
                        let message =
                            format!("`{}` does not live long enough", self.name(&loan.place));
                        self.error(loan.range, message);
                    }
                }
            }
        }
    }

    fn terminator(&mut self, kind: &TerminatorKind, range: TextRange, live_after: &BitSet) {
        self.in_aggregate = false;
        let live = &Live {
            locals: live_after,
            except: None,
            args: &[],
        };
        for operand in kind.operands() {
            self.operand(operand, live, range);
        }
        // A temporary dropped while a reference to it is still to be
        // used (E0716); a binding's scope ending is E0597, at its
        // `StorageDead`.
        if let TerminatorKind::Drop { place, .. } = kind
            && self.body.locals[place.local].kind == LocalKind::Temp
            && !place.has_deref()
            && self.borrowed[place.local.index()].iter().any(|&id| {
                self.is_live(id as usize, live) && overlaps(place, &self.loans[id as usize].0.place)
            })
        {
            self.error(range, "temporary value dropped while borrowed".into());
        }
        let (TerminatorKind::Call { args, dest, .. }
        | TerminatorKind::CallIndirect { args, dest, .. }) = kind
        else {
            return;
        };
        // The loans passed to the call are in use during it, together
        // with those live after it.
        let passed: Vec<Local> = args
            .iter()
            .filter_map(|arg| match arg {
                Operand::Copy(place) | Operand::Move(place) => Some(place.local),
                Operand::Const(_) => None,
            })
            .collect();
        // Activate the two-phase borrows of receivers.
        let made_into = self.made_into;
        for &local in &passed {
            for &id in &made_into[local.index()] {
                let loan = &self.loans[id as usize].0;
                if loan.two_phase {
                    let others = &Live {
                        locals: live_after,
                        except: Some(id as usize),
                        args: &passed,
                    };
                    let place = loan.place.clone();
                    self.access(&place, Access::Borrow(Mutability::Mut), others, range);
                }
            }
        }
        self.write(dest, live, range);
    }

    fn operand(&mut self, operand: &Operand, live: &Live, range: TextRange) {
        match operand {
            Operand::Copy(place) => self.access(place, Access::Read, live, range),
            Operand::Move(place) => {
                if let Some(last) = place
                    .projection
                    .iter()
                    .rposition(|e| matches!(e, ProjectionElem::Index(_)))
                {
                    // Moving out of an element would leave a hole (E0508).
                    let base = Place {
                        local: place.local,
                        projection: place.projection[..last].into(),
                    };
                    let ty = self.body.place_ty(&base, self.sigs);
                    let what = if ty.is_unsized() { "slice" } else { "array" };
                    let message = format!("cannot move out of type `{ty}`, a non-copy {what}");
                    self.error(range, message);
                } else if place.has_deref() {
                    self.check_move_out(place, range);
                } else if let Some(ty) = self.drop_owner(place) {
                    let message = format!(
                        "cannot move out of type `{ty}`, which implements the `Drop` trait"
                    );
                    // Once per field `..base` moves, but once per pattern.
                    if self.in_aggregate {
                        self.diagnostics.push(Diagnostic::new(range, message));
                    } else {
                        self.error(range, message);
                    }
                } else {
                    self.access(place, Access::Move, live, range);
                }
            }
            Operand::Const(_) => {}
        }
    }

    fn write(&mut self, place: &Place, live: &Live, range: TextRange) {
        if let Some((name, false, whole)) = self.static_of(place) {
            let message = if whole {
                format!("cannot assign to immutable static item `{name}`")
            } else {
                format!(
                    "cannot assign to `{}`, as `{name}` is an immutable static item",
                    self.name(place)
                )
            };
            return self.error(range, message);
        }
        if let Some(pointer) = self.last_pointer(place)
            && !matches!(
                pointer.kind(),
                TyKind::Ref(Mutability::Mut, _) | TyKind::RawPtr(Mutability::Mut, _)
            )
        {
            let message = format!(
                "cannot assign to `{}`, which is behind {}",
                self.name(place),
                pointer_kind(pointer)
            );
            return self.error(range, message);
        }
        self.access(place, Access::Write, live, range);
    }

    fn drop_owner(&self, place: &Place) -> Option<Ty> {
        drop_owner(self.body, self.items, self.sigs, place)
    }

    /// The type of the pointer the last dereference in `place` goes
    /// through, if any.
    fn last_pointer(&self, place: &Place) -> Option<Ty> {
        let last = place
            .projection
            .iter()
            .rposition(|&e| e == ProjectionElem::Deref)?;
        let prefix = Place {
            local: place.local,
            projection: place.projection[..last].into(),
        };
        Some(self.body.place_ty(&prefix, self.sigs))
    }

    /// `&mut place` needs a `mut` binding, or a mutable pointer.
    fn check_mutable_borrow(&mut self, place: &Place, range: TextRange) {
        let name = self.name(place);
        if let Some((item, false, whole)) = self.static_of(place) {
            let message = if whole {
                format!("cannot borrow immutable static item `{item}` as mutable")
            } else {
                format!(
                    "cannot borrow `{name}` as mutable, as `{item}` is an immutable static item"
                )
            };
            return self.error(range, message);
        }
        match self.last_pointer(place) {
            Some(pointer) => {
                if !matches!(
                    pointer.kind(),
                    TyKind::Ref(Mutability::Mut, _) | TyKind::RawPtr(Mutability::Mut, _)
                ) {
                    let message = format!(
                        "cannot borrow `{name}` as mutable, as it is behind {}",
                        pointer_kind(pointer)
                    );
                    self.error(range, message);
                }
            }
            None => {
                if let LocalKind::User {
                    mutable: false,
                    name: local,
                } = self.body.locals[place.local].kind
                {
                    // A part of a binding is named with the binding.
                    let what = if place.projection.is_empty() {
                        "it".to_owned()
                    } else {
                        format!("`{local}`")
                    };
                    let message = format!(
                        "cannot borrow `{name}` as mutable, as {what} is not declared as mutable"
                    );
                    self.error(range, message);
                }
            }
        }
    }

    /// Moving out of `place`, which is behind a pointer.
    fn check_move_out(&mut self, place: &Place, range: TextRange) {
        if let Some((item, _, whole)) = self.static_of(place) {
            let message = if whole {
                format!("cannot move out of static item `{item}`")
            } else {
                format!(
                    "cannot move out of `{}` as `{item}` is a static item",
                    self.name(place)
                )
            };
            return self.error(range, message);
        }
        let behind = match self.last_pointer(place).map(|p| p.kind()) {
            Some(TyKind::Ref(Mutability::Not, _)) => "a shared reference",
            Some(TyKind::Ref(Mutability::Mut, _)) => "a mutable reference",
            _ => "a raw pointer",
        };
        // A variant's field is named with the variant: `r` as enum
        // variant `A`.
        if let Some(at) = place
            .projection
            .iter()
            .rposition(|e| matches!(e, crate::ProjectionElem::Downcast(_)))
            && let crate::ProjectionElem::Downcast(variant) = place.projection[at]
        {
            let mut base = Place {
                local: place.local,
                projection: place.projection[..at].into(),
            };
            let ty = self.body.place_ty(&base, self.sigs);
            if let [rest @ .., crate::ProjectionElem::Deref] = &*base.projection {
                base.projection = rest.into();
            }
            let name = match ty.kind() {
                TyKind::Adt(adt, _) => self.items.structs[adt.strukt].variant(variant).name,
                _ => flux_intern::kw::Empty,
            };
            let message = format!(
                "cannot move out of `{}` as enum variant `{name}` which is behind {behind}",
                self.name(&base)
            );
            self.error(range, message);
            return;
        }
        // rustc names a tuple struct rather than its field: `w`, not
        // `w.0`.
        let mut named = place.clone();
        while let [base @ .., crate::ProjectionElem::Field(_)] = &*named.projection {
            let base = Place {
                local: named.local,
                projection: base.into(),
            };
            let is_tuple_struct = match self.body.place_ty(&base, self.sigs).kind() {
                TyKind::Adt(adt, _) => {
                    self.items.structs[adt.strukt].kind == flux_hir::StructKind::Tuple
                }
                _ => false,
            };
            if !is_tuple_struct {
                break;
            }
            named = base;
        }
        // And the reference it is behind: `w`, not `*w`.
        if named.projection.len() < place.projection.len()
            && let [base @ .., crate::ProjectionElem::Deref] = &*named.projection
        {
            named = Place {
                local: named.local,
                projection: base.into(),
            };
        }
        let message = format!(
            "cannot move out of `{}` which is behind {behind}",
            self.name(&named)
        );
        self.error(range, message);
    }

    /// Checks an access to `place` against the `live` loans.
    fn access(&mut self, place: &Place, access: Access, live: &Live, range: TextRange) {
        let borrowed = self.borrowed;
        for &id in &borrowed[place.local.index()] {
            let id = id as usize;
            let loan = &self.loans[id].0;
            if !self.is_live(id, live) || !overlaps(place, &loan.place) {
                continue;
            }
            // A two-phase loan is shared until its call.
            let loan_mut = loan.mutability == Mutability::Mut && !loan.two_phase;
            let name = self.name(place);
            let message = match access {
                Access::Read if loan_mut => {
                    format!("cannot use `{name}` because it was mutably borrowed")
                }
                Access::Read => continue,
                Access::Move => format!("cannot move out of `{name}` because it is borrowed"),
                Access::Write => format!("cannot assign to `{name}` because it is borrowed"),
                Access::Borrow(Mutability::Not) if loan_mut => format!(
                    "cannot borrow `{name}` as immutable because it is also borrowed as mutable"
                ),
                Access::Borrow(Mutability::Not) => continue,
                Access::Reserve if loan_mut => {
                    format!("cannot borrow `{name}` as mutable more than once at a time")
                }
                Access::Reserve => continue,
                Access::Borrow(Mutability::Mut) if loan.mutability == Mutability::Mut => {
                    format!("cannot borrow `{name}` as mutable more than once at a time")
                }
                Access::Borrow(Mutability::Mut) => format!(
                    "cannot borrow `{name}` as mutable because it is also borrowed as immutable"
                ),
            };
            return self.error(range, message);
        }
    }
}

/// The outermost struct `place` is a field of, if it has a `Drop` impl:
/// its fields may not be moved out of (E0509), since its `drop` will
/// see them.
pub(crate) fn drop_owner(
    body: &Body,
    items: &ItemTree,
    sigs: &Signatures,
    place: &Place,
) -> Option<Ty> {
    (0..place.projection.len()).find_map(|len| {
        let base = Place {
            local: place.local,
            projection: place.projection[..len].into(),
        };
        let ty = body.place_ty(&base, sigs);
        match ty.kind() {
            TyKind::Adt(adt, _)
                if flux_ty::traits::drop_impl(items, sigs, adt.strukt).is_some() =>
            {
                Some(ty)
            }
            _ => None,
        }
    })
}

/// "a `&` reference" or "a `*const` pointer".
fn pointer_kind(pointer: Ty) -> &'static str {
    match pointer.kind() {
        TyKind::RawPtr(..) => "a `*const` pointer",
        _ => "a `&` reference",
    }
}
