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
        for (id, &h) in held.iter().enumerate() {
            if h {
                held_by[id].push(Local::from_raw(local as u32));
            }
        }
    }
    let live_out = liveness(body);
    let mut checker = Checker {
        body,
        items,
        sigs,
        loans: &loans,
        holds: &holds,
        held_by: &held_by,
        diagnostics: Vec::new(),
        reported_dead: vec![false; loans.len()],
        in_aggregate: false,
    };
    for (block, data) in body.blocks.iter() {
        // The locals live after each statement, found walking backwards.
        let mut live = live_out[block.index()].clone();
        let mut after = vec![Vec::new(); data.statements.len()];
        let term_live = live.clone();
        terminator_transfer(&data.terminator.kind, &mut live);
        for (i, statement) in data.statements.iter().enumerate().rev() {
            after[i] = live.clone();
            statement_transfer(&statement.kind, &mut live);
        }
        for (i, statement) in data.statements.iter().enumerate() {
            checker.statement(block, i, &statement.kind, statement.range, &after[i]);
        }
        checker.terminator(&data.terminator.kind, data.terminator.range, &term_live);
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

/// For each local, the loans it may hold: those made into it, those of
/// references copied into it, and those of the reference a reborrow goes
/// through.
fn holders(body: &Body, loans: &[(Loan, Local)]) -> Vec<Vec<bool>> {
    let mut holds = vec![vec![false; loans.len()]; body.locals.len()];
    for (id, (_, holder)) in loans.iter().enumerate() {
        holds[holder.index()][id] = true;
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
    let mut changed = true;
    while changed {
        changed = false;
        for &(from, to) in &flows {
            let held = holds[from.index()].clone();
            for (has, held) in holds[to.index()].iter_mut().zip(held) {
                if held && !*has {
                    *has = true;
                    changed = true;
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

/// Updates `live` from after the statement to before it.
fn statement_transfer(kind: &StatementKind, live: &mut [bool]) {
    let mut uses = Vec::new();
    match kind {
        StatementKind::Assign(dest, rvalue) => {
            if dest.projection.is_empty() {
                live[dest.local.index()] = false;
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
        StatementKind::StorageDead(local) => live[local.index()] = false,
        StatementKind::PlaceMention(place) => uses.push(place.local),
    }
    for local in uses {
        live[local.index()] = true;
    }
}

fn terminator_transfer(kind: &TerminatorKind, live: &mut [bool]) {
    if let TerminatorKind::Call { dest, .. } | TerminatorKind::CallIndirect { dest, .. } = kind
        && dest.projection.is_empty()
    {
        live[dest.local.index()] = false;
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
        live[local.index()] = true;
    }
}

/// The locals live at the end of each block.
fn liveness(body: &Body) -> Vec<Vec<bool>> {
    let n = body.locals.len();
    let mut live_in = vec![vec![false; n]; body.blocks.len()];
    let mut live_out = vec![vec![false; n]; body.blocks.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for (block, data) in body.blocks.iter().collect::<Vec<_>>().into_iter().rev() {
            let mut live = vec![false; n];
            for successor in data.terminator.kind.successors() {
                for (l, &v) in live.iter_mut().zip(&live_in[successor.index()]) {
                    *l |= v;
                }
            }
            live_out[block.index()] = live.clone();
            terminator_transfer(&data.terminator.kind, &mut live);
            for statement in data.statements.iter().rev() {
                statement_transfer(&statement.kind, &mut live);
            }
            if live != live_in[block.index()] {
                live_in[block.index()] = live;
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
    holds: &'a [Vec<bool>],
    held_by: &'a [Vec<Local>],
    diagnostics: Vec<Diagnostic>,
    /// Loans already reported for outliving what they borrow.
    reported_dead: Vec<bool>,
    /// Checking the operands of an aggregate.
    in_aggregate: bool,
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

    /// The loans held by the locals in `live`, except `except`.
    fn live_loans(&self, live: &[bool], except: Option<usize>) -> Vec<usize> {
        (0..self.loans.len())
            .filter(|&id| Some(id) != except)
            .filter(|&id| self.held_by[id].iter().any(|h| live[h.index()]))
            .collect()
    }

    fn statement(
        &mut self,
        block: BasicBlock,
        index: usize,
        kind: &StatementKind,
        range: TextRange,
        live_after: &[bool],
    ) {
        match kind {
            StatementKind::Assign(dest, rvalue) => {
                let own = self
                    .loans
                    .iter()
                    .position(|(l, _)| l.block == block && l.statement == index);
                let live = self.live_loans(live_after, own);
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
                        self.access(place, access, &live, range);
                    }
                    // `len` borrows its receiver; a bounds check reads
                    // only the pointer a slice is behind.
                    Rvalue::Len {
                        place,
                        borrows: true,
                    } => self.access(place, Access::Borrow(Mutability::Not), &live, range),
                    Rvalue::Len { .. } => {}
                    _ => {
                        self.in_aggregate = matches!(rvalue, Rvalue::Aggregate(..));
                        for operand in rvalue.operands() {
                            self.operand(operand, &live, range);
                        }
                    }
                }
                self.write(dest, &live, range);
            }
            StatementKind::PlaceMention(_) => {}
            StatementKind::StorageDead(local) => {
                let live = self.live_loans(live_after, None);
                for id in live {
                    let loan = &self.loans[id].0;
                    if loan.place.local == *local
                        && !loan.place.has_deref()
                        && !self.reported_dead[id]
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

    fn terminator(&mut self, kind: &TerminatorKind, range: TextRange, live_after: &[bool]) {
        self.in_aggregate = false;
        let live = self.live_loans(live_after, None);
        for operand in kind.operands() {
            self.operand(operand, &live, range);
        }
        // A temporary dropped while a reference to it is still to be
        // used (E0716); a binding's scope ending is E0597, at its
        // `StorageDead`.
        if let TerminatorKind::Drop { place, .. } = kind
            && self.body.locals[place.local].kind == LocalKind::Temp
            && !place.has_deref()
            && live
                .iter()
                .any(|&id| overlaps(place, &self.loans[id].0.place))
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
        let mut in_use = live.clone();
        for arg in args {
            if let Operand::Copy(place) | Operand::Move(place) = arg {
                let holder = &self.holds[place.local.index()];
                in_use.extend((0..self.loans.len()).filter(|&id| holder[id]));
            }
        }
        in_use.sort_unstable();
        in_use.dedup();
        // Activate the two-phase borrows of receivers.
        for arg in args {
            let (Operand::Copy(place) | Operand::Move(place)) = arg else {
                continue;
            };
            for (id, (loan, holder)) in self.loans.iter().enumerate() {
                if *holder == place.local && loan.two_phase {
                    let others: Vec<usize> = in_use.iter().copied().filter(|&o| o != id).collect();
                    let place = loan.place.clone();
                    self.access(&place, Access::Borrow(Mutability::Mut), &others, range);
                }
            }
        }
        self.write(dest, &live, range);
    }

    fn operand(&mut self, operand: &Operand, live: &[usize], range: TextRange) {
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

    fn write(&mut self, place: &Place, live: &[usize], range: TextRange) {
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
    fn access(&mut self, place: &Place, access: Access, live: &[usize], range: TextRange) {
        for &id in live {
            let loan = &self.loans[id].0;
            if !overlaps(place, &loan.place) {
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
