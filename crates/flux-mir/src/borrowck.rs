//! Borrow checking: a borrow must not conflict with other uses of what it
//! borrows while the reference may still be used.
//!
//! Each `&place` or `&mut place` is a *loan*, whose region [`regions`]
//! infers as rustc's non-lexical lifetimes do: the points where a
//! reference made from it, or a value holding one, may still be used,
//! and those its signature's lifetimes require (a reference returned,
//! or stored where a parameter points). A loan is *in scope* at the
//! points its region contains that are reachable from where it was made
//! without passing a write of what it borrows.
//!
//! While a loan is in scope, its place may not be moved out of (E0505)
//! or assigned (E0506); a mutable loan also forbids reading it (E0503)
//! and borrowing it again (E0499, E0502), and a shared one forbids
//! borrowing it mutably (E0502). Places conflict when one contains the
//! other: `s.a` and `s.b` do not. A *two-phase* loan, a method
//! receiver's auto-borrow, acts as a shared loan until the call
//! activates it. A binding whose scope ends while borrowed does not live
//! long enough (E0597), nor does a temporary (E0716) or a parameter; one
//! whose loan the function returns is E0515.
//!
//! This pass also checks what may be written or borrowed mutably: a
//! binding declared without `mut` (E0596, E0594, the latter for fields;
//! assignments to whole locals are [`init`](crate::init)'s) and anything
//! behind a shared reference or `*const` pointer, and that nothing is
//! moved out from behind a pointer (E0507).

mod regions;

use flux_hir::{Diagnostic, ItemTree};
use flux_syntax::TextRange;
use flux_ty::{Mutability, Signatures, Ty, TyKind};

use crate::bitset::BitSet;
use crate::{
    BasicBlock, Body, Local, LocalKind, Operand, Place, ProjectionElem, Rvalue, StatementKind,
    TerminatorKind, place_name,
};

pub use regions::{ClosureRequirements, Ext, Requirements};

/// The diagnostics for `body`'s conflicting borrows and its writes and
/// moves through pointers, given the requirements of the closures it
/// makes; and if it is a closure's, its requirements.
pub fn check(
    body: &Body,
    items: &ItemTree,
    sigs: &Signatures,
    requirements: &Requirements,
) -> (Vec<Diagnostic>, ClosureRequirements) {
    let loans = collect_loans(body);
    let infos: Vec<regions::LoanInfo<'_>> = loans
        .iter()
        .map(|(loan, _)| regions::LoanInfo {
            place: &loan.place,
            block: loan.block,
            statement: loan.statement,
        })
        .collect();
    let mut regions = regions::infer(body, items, sigs, &infos, requirements);
    // The loans of each local's places, and those made into each local.
    let mut borrowed = vec![Vec::new(); body.locals.len()];
    let mut made_into = vec![Vec::new(); body.locals.len()];
    let mut holders: Vec<Local> = Vec::with_capacity(loans.len());
    for (id, (loan, holder)) in loans.iter().enumerate() {
        borrowed[loan.place.local.index()].push(id as u32);
        made_into[holder.index()].push(id as u32);
        holders.push(*holder);
    }
    let mut checker = Checker {
        body,
        items,
        sigs,
        loans: &loans,
        regions: &regions,
        borrowed: &borrowed,
        made_into: &made_into,
        holders: &holders,
        diagnostics: Vec::new(),
        reported_dead: vec![false; loans.len()],
        in_aggregate: false,
        unique_access: false,
    };
    for (block, data) in body.blocks.iter() {
        for (i, statement) in data.statements.iter().enumerate() {
            let point = regions.points.point(block, i);
            checker.statement(block, i, &statement.kind, statement.range, point);
        }
        let point = regions.points.point(block, data.statements.len());
        checker.terminator(&data.terminator.kind, data.terminator.range, point);
    }
    let mut diagnostics = checker.diagnostics;
    // A two-phase borrow conflicting with an earlier one, now active, is
    // reported where it is made; its activation repeats it.
    let mut seen = Vec::new();
    diagnostics.retain(|d| {
        if !d.message.starts_with("cannot borrow") {
            return true;
        }
        let key = (d.range.start(), d.message.clone());
        let new = !seen.contains(&key);
        seen.push(key);
        new
    });
    // Region errors where no other error is; several lifetimes may fail
    // at one place (`x == y` of an invariant type), each reported.
    let others = diagnostics.len();
    for d in std::mem::take(&mut regions.errors) {
        if !diagnostics[..others].iter().any(|e| e.range == d.range) {
            diagnostics.push(d);
        }
    }
    (diagnostics, std::mem::take(&mut regions.requirements))
}

/// The methods of the program's trait `impl` blocks whose lifetimes ask
/// more than their traits' methods do, which calls through the trait
/// could not know: rustc's "`impl` item signature doesn't match `trait`
/// item signature".
pub fn check_impl_methods(items: &ItemTree, sigs: &Signatures) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (imp, data) in items.impls.iter().skip(items.core.impls as usize) {
        let Some(t) = data.trait_ else { continue };
        for &method in &data.functions {
            let function = &items.functions[method];
            if function.derived.is_some() {
                continue;
            }
            let Some(trait_fn) = items.traits[t].function(items, function.name) else {
                continue;
            };
            if !regions::impl_method_matches(items, sigs, imp, method, trait_fn) {
                let mut d = Diagnostic::new(
                    function.def_range,
                    "`impl` item signature doesn't match `trait` item signature",
                );
                d.file = function.loc.file;
                diagnostics.push(d);
            }
        }
    }
    diagnostics
}

struct Loan {
    place: Place,
    mutability: Mutability,
    two_phase: bool,
    /// A closure's mutable capture of a place behind a `&mut` in a
    /// binding that is not `mut` (`|| *r += 1`): rustc's unique
    /// borrow, whose conflicts it names as such (E0500, E0501, E0524).
    unique: bool,
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
                // A closure's capture outlives what it captures where it
                // uses it, as rustc reports.
                let capture = body
                    .capture_uses
                    .iter()
                    .find(|&&(b, s, _, _)| b == block && s == i);
                let unique = capture.is_some() && is_unique(body, place, *mutability);
                let loan = Loan {
                    place: place.clone(),
                    mutability: *mutability,
                    two_phase: *two_phase,
                    unique,
                    block,
                    statement: i,
                    range: capture.map_or(statement.range, |c| c.2),
                };
                loans.push((loan, dest.local));
            }
        }
    }
    loans
}

/// Whether a closure's capture of `place` by a borrow of `mutability`
/// is rustc's unique borrow: mutable, through a dereference, of a
/// binding that is not `mut`.
fn is_unique(body: &Body, place: &Place, mutability: Mutability) -> bool {
    mutability == Mutability::Mut
        && place.has_deref()
        && matches!(
            body.locals[place.local].kind,
            LocalKind::User { mutable: false, .. }
        )
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
                // Borrowing a value that holds loans uses them, and so
                // does reading the length of a slice behind one.
                Rvalue::Ref { place, .. }
                | Rvalue::Len { place, .. }
                | Rvalue::Discriminant(place) => {
                    place_uses(place, &mut uses);
                    uses.push(place.local);
                }
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

/// The locals live at the end of each block, in order, of those
/// `tracked`: whether one is live depends on its own uses alone, and
/// only those whose types have lifetimes matter. A block's are few, so
/// they are listed rather than a bit each for every local.
fn liveness_where(body: &Body, tracked: &[bool]) -> Vec<Vec<u32>> {
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
                if tracked[local.index()] && !live.set(local.index(), value) && value {
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
    regions: &'a regions::Regions,
    /// The loans of each local's places, in order: those an access to a
    /// place of it may conflict with.
    borrowed: &'a [Vec<u32>],
    /// The loans made into each local, in order.
    made_into: &'a [Vec<u32>],
    /// The local each loan is made into, which holds a two-phase one
    /// until its call.
    holders: &'a [Local],
    diagnostics: Vec<Diagnostic>,
    /// Loans already reported for outliving what they borrow.
    reported_dead: Vec<bool>,
    /// Checking the operands of an aggregate.
    in_aggregate: bool,
    /// The access being checked is a closure's unique capture.
    unique_access: bool,
}

/// The loans in scope at a point, except `except`.
#[derive(Clone, Copy)]
struct Live {
    point: u32,
    except: Option<usize>,
}

/// Whether one place contains the other: they are the same local and
/// one projection is a prefix of the other. Elements of an array at
/// indices in locals may be the same whatever their indices, as rustc
/// assumes; those at constant indices (a slice pattern's) are the same
/// only where the indices may be, as rustc decides.
fn overlaps(a: &Place, b: &Place) -> bool {
    a.local == b.local
        && a.projection
            .iter()
            .zip(b.projection.iter())
            .all(|(&x, &y)| elements_may_overlap(x, y))
}

fn elements_may_overlap(x: ProjectionElem, y: ProjectionElem) -> bool {
    use ProjectionElem::{ConstantIndex, Index, Subslice};
    match (x, y) {
        (Index(_), Index(_) | ConstantIndex { .. } | Subslice { .. })
        | (ConstantIndex { .. } | Subslice { .. }, Index(_))
        | (Subslice { .. }, Subslice { .. }) => true,
        (
            ConstantIndex {
                offset: a,
                min_length: m,
                from_end: e,
            },
            ConstantIndex {
                offset: b,
                min_length: n,
                from_end: f,
            },
        ) => {
            if e == f {
                a == b
            } else {
                // One from the start, one from the end, of at least
                // `min` elements.
                let (begin, end) = if e { (b, a) } else { (a, b) };
                begin >= m.max(n).saturating_sub(end)
            }
        }
        (c @ ConstantIndex { .. }, s @ Subslice { .. })
        | (s @ Subslice { .. }, c @ ConstantIndex { .. }) => {
            let (
                ConstantIndex {
                    offset,
                    min_length,
                    from_end,
                },
                Subslice {
                    from,
                    to,
                    from_end: slice_from_end,
                },
            ) = (c, s)
            else {
                unreachable!()
            };
            match (from_end, slice_from_end) {
                (false, false) => offset >= from && offset < to,
                (true, false) => {
                    let index = min_length.saturating_sub(offset);
                    index >= from && index < to
                }
                (false, true) => offset >= from,
                (true, true) => offset > to,
            }
        }
        _ => x == y,
    }
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

    /// Whether loan `id` is in scope at `live`'s point.
    fn is_live(&self, id: usize, live: &Live) -> bool {
        Some(id) != live.except && self.regions.loan_scopes[id].contains(live.point)
    }

    fn statement(
        &mut self,
        block: BasicBlock,
        index: usize,
        kind: &StatementKind,
        range: TextRange,
        point: u32,
    ) {
        let live = &Live {
            point,
            except: None,
        };
        match kind {
            StatementKind::Assign(dest, rvalue) => {
                match rvalue {
                    Rvalue::Ref {
                        mutability,
                        place,
                        two_phase,
                    } => {
                        if *mutability == Mutability::Mut {
                            let capture = self
                                .body
                                .capture_uses
                                .iter()
                                .find(|&&(b, s, _, _)| b == block && s == index);
                            match capture {
                                Some(&(_, _, at, assigned)) => {
                                    self.check_capture_borrow(place, at, assigned);
                                }
                                None => self.check_mutable_borrow(place, range),
                            }
                            self.unique_access =
                                capture.is_some() && is_unique(self.body, place, *mutability);
                        }
                        // A two-phase borrow is shared until the call.
                        let access = if *two_phase {
                            Access::Reserve
                        } else {
                            Access::Borrow(*mutability)
                        };
                        self.access(place, access, live, range);
                        self.unique_access = false;
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
                let borrowed = self.borrowed;
                for &id in &borrowed[local.index()] {
                    let id = id as usize;
                    if self.is_live(id, live) && !self.loans[id].0.place.has_deref() {
                        self.dead(id, range);
                    }
                }
            }
        }
    }

    fn terminator(&mut self, kind: &TerminatorKind, range: TextRange, point: u32) {
        self.in_aggregate = false;
        let live = &Live {
            point,
            except: None,
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
        {
            let borrowed = self.borrowed;
            let dead = borrowed[place.local.index()].iter().find(|&&id| {
                self.is_live(id as usize, live) && overlaps(place, &self.loans[id as usize].0.place)
            });
            if let Some(&id) = dead {
                self.dead(id as usize, range);
            }
        }
        // What the function returns may not borrow what dies with it:
        // its parameters and temporaries (bindings die before).
        // A constant's temporaries in its value live forever.
        if let TerminatorKind::Return = kind
            && self.body.owner.is_some()
        {
            for id in 0..self.loans.len() {
                let place = &self.loans[id].0.place;
                let local = self.body.locals[place.local].kind;
                if !place.has_deref()
                    && !matches!(local, LocalKind::Static(_) | LocalKind::Return)
                    && self.is_live(id, live)
                {
                    self.dead(id, range);
                }
            }
        }
        let (TerminatorKind::Call { args, dest, .. }
        | TerminatorKind::CallIndirect { args, dest, .. }) = kind
        else {
            return;
        };
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
                        point,
                        except: Some(id as usize),
                    };
                    let place = loan.place.clone();
                    self.access(&place, Access::Borrow(Mutability::Mut), others, range);
                }
            }
        }
        self.write(dest, live, range);
    }

    /// Loan `id` is in scope where what it borrows dies, at `range`:
    /// E0515 if it does because the function returns it, otherwise
    /// E0597, or E0716 for a temporary.
    fn dead(&mut self, id: usize, range: TextRange) {
        if self.reported_dead[id] {
            return;
        }
        self.reported_dead[id] = true;
        let loan = &self.loans[id].0;
        let local = loan.place.local;
        let kind = self.body.locals[local].kind;
        let is_param = self.body.is_param(local);
        // A closure's borrow of what it captures, which must outlive the
        // function: rustc's E0373, at the closure.
        if self.regions.escapes[id].is_some()
            && self.body.upvars.is_none()
            && let Some(closure) = self.capturing_closure(id)
        {
            let message = format!(
                "closure may outlive the current function, but it borrows `{}`, which is owned by the current function",
                self.name(&loan.place)
            );
            self.error(closure, message);
            return;
        }
        if let Some((regions::Category::Return, at)) = self.regions.escapes[id]
            && self.body.owner.is_some()
        {
            let reference = if at == loan.range {
                "reference to"
            } else {
                "value referencing"
            };
            let what = match kind {
                LocalKind::User { .. } | LocalKind::Param if is_param => match kind {
                    LocalKind::User { .. } => {
                        format!("function parameter `{}`", self.name(&loan.place))
                    }
                    _ => "function parameter".to_owned(),
                },
                LocalKind::User { .. } if loan.place.projection.is_empty() => {
                    format!("local variable `{}`", self.name(&loan.place))
                }
                LocalKind::User { .. } => format!("local data `{}`", self.name(&loan.place)),
                _ => "temporary value".to_owned(),
            };
            let message = format!("cannot return {reference} {what}");
            self.error(at, message);
            return;
        }
        if kind == LocalKind::Temp {
            self.error(range, "temporary value dropped while borrowed".into());
            return;
        }
        let message = format!("`{}` does not live long enough", self.name(&loan.place));
        self.error(loan.range, message);
    }

    /// Where the closure is made whose capture loan `id` is, if it is one.
    fn capturing_closure(&self, id: usize) -> Option<TextRange> {
        let (loan, temp) = &self.loans[id];
        let body = self.body;
        body.capture_uses
            .iter()
            .find(|&&(b, s, _, _)| b == loan.block && s == loan.statement)?;
        body.blocks[loan.block]
            .statements
            .iter()
            .skip(loan.statement + 1)
            .find_map(|statement| match &statement.kind {
                StatementKind::Assign(_, Rvalue::Aggregate(crate::AggregateKind::Closure, ops))
                    if ops.iter().any(|op| match op {
                        Operand::Move(p) | Operand::Copy(p) => p.local == *temp,
                        Operand::Const(_) => false,
                    }) =>
                {
                    Some(statement.range)
                }
                _ => None,
            })
    }

    fn operand(&mut self, operand: &Operand, live: &Live, range: TextRange) {
        match operand {
            Operand::Copy(place) => self.access(place, Access::Read, live, range),
            Operand::Move(place) => {
                // An element at an index in a local, or a slice pattern's
                // element of a slice (an array's are places of their own).
                let hole = place
                    .projection
                    .iter()
                    .enumerate()
                    .rposition(|(i, e)| match e {
                        ProjectionElem::Index(_) => true,
                        ProjectionElem::ConstantIndex { .. } | ProjectionElem::Subslice { .. } => {
                            let base = Place {
                                local: place.local,
                                projection: place.projection[..i].into(),
                            };
                            self.body.place_ty(&base, self.sigs).is_unsized()
                        }
                        _ => false,
                    });
                if let Some(last) = hole {
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
        if let Some(message) = self.captured_write(place, "assign to") {
            return self.error(range, message);
        }
        if let Some(pointer) = self.immutable_pointer(place)
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

    /// In a closure's body, why `place`, in what it captures, may not be
    /// changed (`what`, "assign to" or "borrow"): a captured variable in
    /// an `Fn` closure, or one captured by value from a binding without
    /// `mut`.
    fn captured_write(&self, place: &Place, what: &str) -> Option<String> {
        let upvars = self.body.upvars.as_ref()?;
        let (field, len) = upvars.of(place)?;
        // Through a reference the captured place holds, the reference
        // decides.
        if place.projection[len..].contains(&ProjectionElem::Deref) {
            return None;
        }
        let name = self.name(place);
        let suffix = if what == "borrow" { " as mutable" } else { "" };
        if upvars.kind == flux_ty::ClosureKind::Fn {
            return Some(format!(
                "cannot {what} `{name}`{suffix}, as it is a captured variable in a `Fn` closure"
            ));
        }
        if !upvars.by_ref[field] && !upvars.mutable[field] {
            let captured = &upvars.names[field];
            let it = if *captured == name {
                "it".to_owned()
            } else {
                format!("`{captured}`")
            };
            return Some(format!(
                "cannot {what} `{name}`{suffix}, as {it} is not declared as mutable"
            ));
        }
        None
    }

    /// The type of the first pointer `place` goes through that does not
    /// allow changing what is behind it, if any: writing through
    /// `**r` needs both references to be mutable.
    fn immutable_pointer(&self, place: &Place) -> Option<Ty> {
        for (i, &elem) in place.projection.iter().enumerate() {
            if elem != ProjectionElem::Deref {
                continue;
            }
            let prefix = Place {
                local: place.local,
                projection: place.projection[..i].into(),
            };
            let pointer = self.body.place_ty(&prefix, self.sigs);
            if !matches!(
                pointer.kind(),
                TyKind::Ref(Mutability::Mut, _) | TyKind::RawPtr(Mutability::Mut, _)
            ) {
                return Some(pointer);
            }
        }
        None
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

    /// A closure's mutable borrow of `place`, used at `range` in it,
    /// where rustc reports one of a binding without `mut` as the use:
    /// "cannot assign to `x`" or "cannot borrow `x` as mutable".
    fn check_capture_borrow(&mut self, place: &Place, range: TextRange, assigned: bool) {
        let before = self.diagnostics.len();
        self.check_mutable_borrow(place, range);
        if assigned && let Some(d) = self.diagnostics.get_mut(before) {
            d.message = d
                .message
                .replacen("cannot borrow", "cannot assign to", 1)
                .replacen("` as mutable,", "`,", 1);
        }
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
        if let Some(message) = self.captured_write(place, "borrow") {
            return self.error(range, message);
        }
        match self.last_pointer(place) {
            Some(_) => {
                if let Some(pointer) = self.immutable_pointer(place) {
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
        if let Some(upvars) = &self.body.upvars
            && let Some((_, len)) = upvars.of(place)
            && !place.projection[len..].contains(&ProjectionElem::Deref)
        {
            let message = format!(
                "cannot move out of `{}`, a captured variable in an `{}` closure",
                self.name(place),
                upvars.kind.name()
            );
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
            // Overwriting a pointer leaves what it points to, and the
            // loans of that, alone (rustc's shallow write).
            if access == Access::Write
                && loan.place.projection.get(place.projection.len()) == Some(&ProjectionElem::Deref)
            {
                continue;
            }
            // A two-phase loan is shared until its call, while what
            // holds it is still to be passed.
            let reserved = loan.two_phase
                && self
                    .regions
                    .liveness
                    .get(self.holders[id].index())
                    .is_some_and(|l| l.contains(live.point));
            let loan_mut = loan.mutability == Mutability::Mut && !reserved;
            let name = self.name(place);
            let borrow = matches!(access, Access::Borrow(_) | Access::Reserve);
            let message = match access {
                _ if borrow && self.unique_access && loan.unique => {
                    format!("two closures require unique access to `{name}` at the same time")
                }
                _ if borrow && self.unique_access => {
                    format!("closure requires unique access to `{name}` but it is already borrowed")
                }
                Access::Borrow(Mutability::Not) if loan.unique => format!(
                    "cannot borrow `{name}` as immutable because previous closure requires unique access"
                ),
                Access::Borrow(Mutability::Mut) | Access::Reserve if loan.unique => format!(
                    "cannot borrow `{name}` as mutable because previous closure requires unique access"
                ),
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
