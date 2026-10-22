//! Definite initialization and moves: every place is assigned, and not
//! moved out of, before it is read or borrowed, and an immutable local is
//! assigned at most once.
//!
//! The analysis tracks *move paths*: each user local and parameter, and
//! every field path into one that the body mentions (`s`, `s.inner`,
//! `s.inner.x`, and through a variant, `(o as Some).0`). Paths stop at a dereference: what a reference points to
//! is not the function's to initialize or move (moving out of it is
//! E0507, in [`borrowck`](crate::borrowck)), so using `(*r).a` uses `r`.
//!
//! Three forward dataflow analyses over the control-flow graph, all
//! joined by union, give for each path whether it *may be uninitialized*
//! (never assigned, moved out of, or out of scope), *may be moved* (to
//! tell a use after move, E0382, from a use of an uninitialized binding,
//! E0381) and *may be initialized* (assigning an immutable local again is
//! E0384). Moving or assigning a path does the same to every path below
//! it. Temporaries are initialized by construction, and parameters on
//! entry. Unreachable blocks are never analyzed, as in rustc.

use std::collections::HashMap;

use flux_hir::{Diagnostic, ItemTree};
use flux_syntax::TextRange;
use flux_ty::Signatures;

use crate::{
    Body, Local, LocalKind, Operand, Place, ProjectionElem, Rvalue, StatementKind, TerminatorKind,
    place_name,
};

/// The diagnostics for `body`'s uses of uninitialized or moved places
/// and its invalid assignments.
pub fn check(body: &Body, items: &ItemTree, sigs: &Signatures) -> Vec<Diagnostic> {
    let mut paths = MovePaths::new(body, |local| {
        body.is_param(local) || matches!(body.locals[local].kind, LocalKind::User { .. })
    });
    paths.forbid_moves(body, items, sigs);
    let entry = analyze(body, &paths, false);

    // Report, walking each reached block from its entry state.
    let mut report = Report {
        body,
        items,
        sigs,
        paths: &paths,
        reported: vec![false; body.locals.len()],
        moved_uses: HashMap::new(),
        diagnostics: Vec::new(),
    };
    for (block, data) in body.blocks.iter() {
        let Some(mut state) = entry[block.index()].clone() else {
            continue;
        };
        for statement in &data.statements {
            for event in statement_events(&statement.kind) {
                report.check(&event, &state, statement.range);
                paths.apply(&mut state, &event, false);
            }
        }
        for event in terminator_events(&data.terminator.kind) {
            report.check(&event, &state, data.terminator.range);
            paths.apply(&mut state, &event, false);
        }
    }
    report.diagnostics
}

/// The state of `paths` on entry to each block, `None` for blocks never
/// reached. With `drop_moves`, a drop moves out of its place, as it does
/// for [`drops`](crate::drops); the check ignores drops, which run only
/// on what is initialized.
pub(crate) fn analyze(body: &Body, paths: &MovePaths, drop_moves: bool) -> Vec<Option<State>> {
    let n = paths.paths.len();
    let mut entry: Vec<Option<State>> = vec![None; body.blocks.len()];
    let mut start = State {
        maybe_uninit: vec![false; n],
        maybe_moved: vec![false; n],
        maybe_init: vec![true; n],
        maybe_held: vec![true; n],
    };
    for (id, path) in paths.paths.iter().enumerate() {
        if !body.is_param(path.local) {
            start.maybe_uninit[id] = true;
            start.maybe_init[id] = false;
            start.maybe_held[id] = false;
        }
    }
    entry[0] = Some(start);
    let mut worklist = vec![Body::entry()];
    while let Some(block) = worklist.pop() {
        let mut state = entry[block.index()]
            .clone()
            .expect("queued blocks are reached");
        let data = &body.blocks[block];
        for statement in &data.statements {
            for event in statement_events(&statement.kind) {
                paths.apply(&mut state, &event, drop_moves);
            }
        }
        for event in terminator_events(&data.terminator.kind) {
            paths.apply(&mut state, &event, drop_moves);
        }
        for successor in data.terminator.kind.successors() {
            let changed = match &mut entry[successor.index()] {
                Some(existing) => existing.join(&state),
                slot @ None => {
                    *slot = Some(state.clone());
                    true
                }
            };
            if changed {
                worklist.push(successor);
            }
        }
    }
    entry
}

/// What a statement or terminator does to places, in order.
pub(crate) enum Event<'a> {
    Read(&'a Place),
    Move(&'a Place),
    Borrow(&'a Place),
    Assign(&'a Place),
    StorageDead(Local),
    Drop(&'a Place),
}

fn operand_event(operand: &Operand) -> Option<Event<'_>> {
    match operand {
        Operand::Copy(place) => Some(Event::Read(place)),
        Operand::Move(place) => Some(Event::Move(place)),
        Operand::Const(_) => None,
    }
}

pub(crate) fn statement_events(kind: &StatementKind) -> Vec<Event<'_>> {
    match kind {
        StatementKind::Assign(dest, rvalue) => {
            let mut events: Vec<Event> = match rvalue {
                Rvalue::Ref { place, .. } => vec![Event::Borrow(place)],
                // Which variant a value holds is a read of all of it.
                Rvalue::Discriminant(place) => vec![Event::Read(place)],
                _ => rvalue
                    .operands()
                    .into_iter()
                    .filter_map(operand_event)
                    .collect(),
            };
            events.push(Event::Assign(dest));
            events
        }
        StatementKind::StorageDead(local) => vec![Event::StorageDead(*local)],
        StatementKind::PlaceMention(_) => Vec::new(),
    }
}

pub(crate) fn terminator_events(kind: &TerminatorKind) -> Vec<Event<'_>> {
    let mut events: Vec<Event> = kind
        .operands()
        .into_iter()
        .filter_map(operand_event)
        .collect();
    match kind {
        TerminatorKind::Call { dest, .. } | TerminatorKind::CallIndirect { dest, .. } => {
            events.push(Event::Assign(dest))
        }
        TerminatorKind::Drop { place, .. } => events.push(Event::Drop(place)),
        _ => {}
    }
    events
}

/// A local the analysis tracks, or a path of fields (and variants) into
/// one.
pub(crate) struct MovePath {
    pub(crate) local: Local,
    pub(crate) fields: Box<[ProjectionElem]>,
    pub(crate) parent: Option<usize>,
    pub(crate) children: Vec<usize>,
}

pub(crate) struct MovePaths {
    pub(crate) paths: Vec<MovePath>,
    ids: HashMap<(Local, Box<[ProjectionElem]>), usize>,
    /// Moves out of a struct with a `Drop` impl, E0509: they move
    /// nothing, as in rustc, so later uses are not errors too.
    forbidden: Vec<Place>,
}

/// The fields and variants of `place` before its first dereference or
/// index, and whether it has one: the elements of an array are not
/// tracked apart, as rustc's move paths stop at them (moving out of one
/// is E0508).
fn fields_before_deref(place: &Place) -> (Box<[ProjectionElem]>, bool) {
    match place
        .projection
        .iter()
        .position(|&e| matches!(e, ProjectionElem::Deref | ProjectionElem::Index(_)))
    {
        Some(deref) => (place.projection[..deref].into(), true),
        None => (place.projection.clone(), false),
    }
}

impl MovePaths {
    /// The paths of `body`'s locals that `tracked` says, and of every
    /// place into them that it mentions.
    pub(crate) fn new(body: &Body, tracked: impl Fn(Local) -> bool) -> MovePaths {
        let mut paths = MovePaths {
            paths: Vec::new(),
            ids: HashMap::new(),
            forbidden: Vec::new(),
        };
        for (local, _) in body.locals.iter() {
            if tracked(local) {
                paths.insert(local, &[]);
            }
        }
        let mut add = |place: &Place| {
            if paths.ids.contains_key(&(place.local, Box::default())) {
                paths.insert(place.local, &fields_before_deref(place).0);
            }
        };
        for data in body.blocks.values() {
            let statement_events = data
                .statements
                .iter()
                .flat_map(|s| statement_events(&s.kind));
            for event in statement_events.chain(terminator_events(&data.terminator.kind)) {
                match event {
                    Event::Read(place)
                    | Event::Move(place)
                    | Event::Borrow(place)
                    | Event::Assign(place)
                    | Event::Drop(place) => add(place),
                    Event::StorageDead(_) => {}
                }
            }
        }
        paths
    }

    /// Makes moves out of structs with `Drop` impls (E0509) move nothing.
    fn forbid_moves(&mut self, body: &Body, items: &ItemTree, sigs: &Signatures) {
        for data in body.blocks.values() {
            let events = data
                .statements
                .iter()
                .flat_map(|s| statement_events(&s.kind))
                .chain(terminator_events(&data.terminator.kind));
            for event in events {
                if let Event::Move(place) = event
                    && !place.has_deref()
                    && crate::borrowck::drop_owner(body, items, sigs, place).is_some()
                {
                    self.forbidden.push(place.clone());
                }
            }
        }
    }

    /// The path for `local` with `fields` and its prefixes.
    fn insert(&mut self, local: Local, fields: &[ProjectionElem]) -> usize {
        if let Some(&id) = self.ids.get(&(local, fields.into())) {
            return id;
        }
        let parent = fields
            .split_last()
            .map(|(_, prefix)| self.insert(local, prefix));
        let id = self.paths.len();
        self.paths.push(MovePath {
            local,
            fields: fields.into(),
            parent,
            children: Vec::new(),
        });
        if let Some(parent) = parent {
            self.paths[parent].children.push(id);
        }
        self.ids.insert((local, fields.into()), id);
        id
    }

    /// The path of `place`, or of the pointer it is behind, and whether
    /// it is behind one.
    pub(crate) fn find(&self, place: &Place) -> Option<(usize, bool)> {
        let (fields, behind_deref) = fields_before_deref(place);
        let path = self.ids.get(&(place.local, fields)).copied()?;
        Some((path, behind_deref))
    }

    /// `path` and every path below it.
    pub(crate) fn subtree(&self, path: usize, out: &mut Vec<usize>) {
        out.push(path);
        for &child in &self.paths[path].children {
            self.subtree(child, out);
        }
    }

    /// The paths above `path`, nearest first.
    fn ancestors(&self, path: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.paths[path].parent, |&p| self.paths[p].parent)
    }

    pub(crate) fn apply(&self, state: &mut State, event: &Event, drop_moves: bool) {
        let (place, effect): (Option<&Place>, fn(&mut State, usize)) = match event {
            Event::Move(place) if self.forbidden.contains(place) => return,
            Event::Move(place) | Event::Drop(place)
                if drop_moves || matches!(event, Event::Move(_)) =>
            {
                (Some(place), |state, p| {
                    state.maybe_uninit[p] = true;
                    state.maybe_moved[p] = true;
                    state.maybe_held[p] = false;
                })
            }
            Event::Drop(_) => return,
            Event::Move(_) => unreachable!("matched above"),
            Event::Assign(place) => (Some(place), |state, p| {
                state.maybe_uninit[p] = false;
                state.maybe_moved[p] = false;
                state.maybe_init[p] = true;
                state.maybe_held[p] = true;
            }),
            Event::StorageDead(local) => {
                if let Some(&root) = self.ids.get(&(*local, Box::default())) {
                    let mut subtree = Vec::new();
                    self.subtree(root, &mut subtree);
                    for p in subtree {
                        state.maybe_uninit[p] = true;
                        state.maybe_moved[p] = false;
                        state.maybe_init[p] = false;
                        state.maybe_held[p] = false;
                    }
                }
                return;
            }
            Event::Read(_) | Event::Borrow(_) => return,
        };
        // Moving out of or assigning through a pointer leaves the pointer
        // as it was.
        if let Some((path, false)) = place.and_then(|place| self.find(place)) {
            let mut subtree = Vec::new();
            self.subtree(path, &mut subtree);
            for p in subtree {
                effect(state, p);
            }
        }
    }
}

/// Reports errors: of the uses of uninitialized places, one per local;
/// of the uses of moved ones, one per set of moved paths they find, as
/// rustc reports one per set of moves, the use of the most specific
/// place.
struct Report<'a> {
    body: &'a Body,
    items: &'a ItemTree,
    sigs: &'a Signatures,
    paths: &'a MovePaths,
    reported: Vec<bool>,
    /// For each set of moved paths a use found, its diagnostic and the
    /// length of the place used.
    moved_uses: HashMap<Vec<usize>, (usize, usize)>,
    diagnostics: Vec<Diagnostic>,
}

impl Report<'_> {
    fn error(&mut self, range: TextRange, message: String) {
        self.diagnostics.push(Diagnostic::new(range, message));
    }

    fn check(&mut self, event: &Event, state: &State, range: TextRange) {
        match event {
            Event::Read(place) | Event::Move(place) => self.check_use(place, false, state, range),
            Event::Borrow(place) => self.check_use(place, true, state, range),
            Event::Assign(place) => self.check_assign(place, state, range),
            Event::StorageDead(_) | Event::Drop(_) => {}
        }
    }

    /// A read, move or borrow of `place`, which must be initialized.
    fn check_use(&mut self, place: &Place, borrow: bool, state: &State, range: TextRange) {
        let Some((path, behind_deref)) = self.paths.find(place) else {
            return;
        };
        if self.reported[place.local.index()] {
            return;
        }
        let mut below = Vec::new();
        // Through a pointer only the pointer itself is used.
        if !behind_deref {
            self.paths.subtree(path, &mut below);
        } else {
            below.push(path);
        }
        let above: Vec<usize> = self.paths.ancestors(path).collect();
        let used = if borrow { "borrow" } else { "use" };
        // The outermost moved path containing the place, if any.
        let moved = above
            .iter()
            .rev()
            .chain([&path])
            .find(|&&p| state.maybe_moved[p]);
        let moves: Vec<usize> = match moved {
            Some(&moved) => vec![moved],
            None => below
                .iter()
                .copied()
                .filter(|&p| state.maybe_moved[p])
                .collect(),
        };
        if !moves.is_empty() {
            // rustc does not name places inside variants.
            let named = |path: usize| {
                let name = self.path_name(path);
                let in_variant = self.paths.paths[path]
                    .fields
                    .iter()
                    .any(|e| matches!(e, ProjectionElem::Downcast(_)));
                if in_variant {
                    String::new()
                } else {
                    format!(": `{name}`")
                }
            };
            let message = match moved {
                Some(&moved) => format!("{used} of moved value{}", named(moved)),
                None => format!("{used} of partially moved value{}", named(path)),
            };
            let specific = place.projection.len();
            match self.moved_uses.get(&moves).copied() {
                Some((_, length)) if length >= specific => {}
                Some((index, _)) => {
                    self.diagnostics[index] = Diagnostic::new(range, message);
                    self.moved_uses.insert(moves, (index, specific));
                }
                None => {
                    self.moved_uses
                        .insert(moves, (self.diagnostics.len(), specific));
                    self.error(range, message);
                }
            }
            return;
        }
        let message = if above.iter().chain(&below).any(|&p| state.maybe_uninit[p]) {
            let root = self.local_name(place.local);
            let how = if state.maybe_init[path] {
                "is possibly-uninitialized"
            } else {
                "isn't initialized"
            };
            format!("used binding `{root}` {how}")
        } else {
            return;
        };
        self.reported[place.local.index()] = true;
        self.error(range, message);
    }

    fn check_assign(&mut self, place: &Place, state: &State, range: TextRange) {
        let Some((path, behind_deref)) = self.paths.find(place) else {
            return;
        };
        if behind_deref {
            // Assigning through a pointer reads the pointer; whether it
            // may be written through is the borrow checker's question.
            // An element of a local's array needs the array, and a `mut`
            // binding.
            let path = &self.paths.paths[path];
            let element = place.projection.get(path.fields.len()) != Some(&ProjectionElem::Deref);
            let before = self.diagnostics.len();
            self.check_use(place, false, state, range);
            if element
                && self.diagnostics.len() == before
                && let LocalKind::User {
                    name,
                    mutable: false,
                } = self.body.locals[place.local].kind
            {
                let place = place_name(self.body, self.items, self.sigs, place);
                self.error(
                    range,
                    format!("cannot assign to `{place}`, as `{name}` is not declared as mutable"),
                );
            }
            return;
        }
        let LocalKind::User { name, mutable } = self.body.locals[place.local].kind else {
            return;
        };
        if place.projection.is_empty() {
            if !mutable && state.maybe_init[path] {
                self.error(
                    range,
                    format!("cannot assign twice to immutable variable `{name}`"),
                );
            }
            return;
        }
        let above: Vec<usize> = self.paths.ancestors(path).collect();
        let message = if let Some(&moved) = above.iter().rev().find(|&&p| state.maybe_moved[p]) {
            format!("assign to part of moved value: `{}`", self.path_name(moved))
        } else if above.iter().any(|&p| state.maybe_uninit[p]) {
            // Once per binding: assigning its other fields is the same
            // mistake.
            if std::mem::replace(&mut self.reported[place.local.index()], true) {
                return;
            }
            format!("partially assigned binding `{name}` isn't fully initialized")
        } else if !mutable {
            let place = place_name(self.body, self.items, self.sigs, place);
            format!("cannot assign to `{place}`, as `{name}` is not declared as mutable")
        } else {
            return;
        };
        self.error(range, message);
    }

    fn local_name(&self, local: Local) -> String {
        place_name(self.body, self.items, self.sigs, &local.into())
    }

    /// The place of a move path, as rustc writes it: `s.inner.0`.
    fn path_name(&self, path: usize) -> String {
        let path = &self.paths.paths[path];
        let place = Place {
            local: path.local,
            projection: path.fields.clone(),
        };
        place_name(self.body, self.items, self.sigs, &place)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) maybe_uninit: Vec<bool>,
    pub(crate) maybe_moved: Vec<bool>,
    /// Ever assigned: what assigning an immutable local again needs.
    pub(crate) maybe_init: Vec<bool>,
    /// Holding a value now: assigned, and not moved out of since.
    pub(crate) maybe_held: Vec<bool>,
}

impl State {
    /// Unions `other` into `self`; whether anything changed.
    fn join(&mut self, other: &State) -> bool {
        let mut changed = false;
        let mine = self
            .maybe_uninit
            .iter_mut()
            .chain(self.maybe_moved.iter_mut())
            .chain(self.maybe_init.iter_mut())
            .chain(self.maybe_held.iter_mut());
        let theirs = other
            .maybe_uninit
            .iter()
            .chain(&other.maybe_moved)
            .chain(&other.maybe_init)
            .chain(&other.maybe_held);
        for (a, &b) in mine.zip(theirs) {
            if b && !*a {
                *a = true;
                changed = true;
            }
        }
        changed
    }
}
