//! Definite initialization and moves: every place is assigned, and not
//! moved out of, before it is read or borrowed, and an immutable local is
//! assigned at most once.
//!
//! The analysis tracks *move paths*: each user local and parameter, and
//! every field path into one that the body mentions (`s`, `s.inner`,
//! `s.inner.x`). Paths stop at a dereference: what a reference points to
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
    let paths = MovePaths::new(body);
    let n = paths.paths.len();

    // State on entry to each block, `None` until reached.
    let mut entry: Vec<Option<State>> = vec![None; body.blocks.len()];
    let mut start = State {
        maybe_uninit: vec![false; n],
        maybe_moved: vec![false; n],
        maybe_init: vec![true; n],
    };
    for (id, path) in paths.paths.iter().enumerate() {
        if !body.is_param(path.local) {
            start.maybe_uninit[id] = true;
            start.maybe_init[id] = false;
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
                paths.apply(&mut state, &event);
            }
        }
        for event in terminator_events(&data.terminator.kind) {
            paths.apply(&mut state, &event);
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

    // Report, walking each reached block from its entry state.
    let mut report = Report {
        body,
        items,
        sigs,
        paths: &paths,
        reported: vec![false; body.locals.len()],
        diagnostics: Vec::new(),
    };
    for (block, data) in body.blocks.iter() {
        let Some(mut state) = entry[block.index()].clone() else {
            continue;
        };
        for statement in &data.statements {
            for event in statement_events(&statement.kind) {
                report.check(&event, &state, statement.range);
                paths.apply(&mut state, &event);
            }
        }
        for event in terminator_events(&data.terminator.kind) {
            report.check(&event, &state, data.terminator.range);
            paths.apply(&mut state, &event);
        }
    }
    report.diagnostics
}

/// What a statement or terminator does to places, in order.
enum Event<'a> {
    Read(&'a Place),
    Move(&'a Place),
    Borrow(&'a Place),
    Assign(&'a Place),
    StorageDead(Local),
}

fn operand_event(operand: &Operand) -> Option<Event<'_>> {
    match operand {
        Operand::Copy(place) => Some(Event::Read(place)),
        Operand::Move(place) => Some(Event::Move(place)),
        Operand::Const(_) => None,
    }
}

fn statement_events(kind: &StatementKind) -> Vec<Event<'_>> {
    match kind {
        StatementKind::Assign(dest, rvalue) => {
            let mut events: Vec<Event> = match rvalue {
                Rvalue::Ref { place, .. } => vec![Event::Borrow(place)],
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

fn terminator_events(kind: &TerminatorKind) -> Vec<Event<'_>> {
    let mut events: Vec<Event> = kind
        .operands()
        .into_iter()
        .filter_map(operand_event)
        .collect();
    if let TerminatorKind::Call { dest, .. } = kind {
        events.push(Event::Assign(dest));
    }
    events
}

/// A user local or parameter, or a path of fields into one.
struct MovePath {
    local: Local,
    fields: Box<[u32]>,
    parent: Option<usize>,
    children: Vec<usize>,
}

struct MovePaths {
    paths: Vec<MovePath>,
    ids: HashMap<(Local, Box<[u32]>), usize>,
}

/// The fields of `place` before its first dereference, and whether it
/// has one.
fn fields_before_deref(place: &Place) -> (Box<[u32]>, bool) {
    let mut fields = Vec::new();
    for elem in &place.projection {
        match elem {
            ProjectionElem::Field(field) => fields.push(*field),
            ProjectionElem::Deref => return (fields.into(), true),
        }
    }
    (fields.into(), false)
}

impl MovePaths {
    /// The paths of `body`'s user locals and parameters, and of every
    /// place into them that it mentions.
    fn new(body: &Body) -> MovePaths {
        let mut paths = MovePaths {
            paths: Vec::new(),
            ids: HashMap::new(),
        };
        for (local, decl) in body.locals.iter() {
            if body.is_param(local) || matches!(decl.kind, LocalKind::User { .. }) {
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
                    | Event::Assign(place) => add(place),
                    Event::StorageDead(_) => {}
                }
            }
        }
        paths
    }

    /// The path for `local` with `fields` and its prefixes.
    fn insert(&mut self, local: Local, fields: &[u32]) -> usize {
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
    fn find(&self, place: &Place) -> Option<(usize, bool)> {
        let (fields, behind_deref) = fields_before_deref(place);
        let path = self.ids.get(&(place.local, fields)).copied()?;
        Some((path, behind_deref))
    }

    /// `path` and every path below it.
    fn subtree(&self, path: usize, out: &mut Vec<usize>) {
        out.push(path);
        for &child in &self.paths[path].children {
            self.subtree(child, out);
        }
    }

    /// The paths above `path`, nearest first.
    fn ancestors(&self, path: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.paths[path].parent, |&p| self.paths[p].parent)
    }

    fn apply(&self, state: &mut State, event: &Event) {
        let (place, effect): (Option<&Place>, fn(&mut State, usize)) = match event {
            Event::Move(place) => (Some(place), |state, p| {
                state.maybe_uninit[p] = true;
                state.maybe_moved[p] = true;
            }),
            Event::Assign(place) => (Some(place), |state, p| {
                state.maybe_uninit[p] = false;
                state.maybe_moved[p] = false;
                state.maybe_init[p] = true;
            }),
            Event::StorageDead(local) => {
                if let Some(&root) = self.ids.get(&(*local, Box::default())) {
                    let mut subtree = Vec::new();
                    self.subtree(root, &mut subtree);
                    for p in subtree {
                        state.maybe_uninit[p] = true;
                        state.maybe_moved[p] = false;
                        state.maybe_init[p] = false;
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

/// Reports errors, a use of each local at most once.
struct Report<'a> {
    body: &'a Body,
    items: &'a ItemTree,
    sigs: &'a Signatures,
    paths: &'a MovePaths,
    reported: Vec<bool>,
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
            Event::StorageDead(_) => {}
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
        let message = if let Some(&moved) = moved {
            format!("{used} of moved value: `{}`", self.path_name(moved))
        } else if below.iter().any(|&p| state.maybe_moved[p]) {
            format!(
                "{used} of partially moved value: `{}`",
                self.path_name(path)
            )
        } else if above.iter().chain(&below).any(|&p| state.maybe_uninit[p]) {
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
            return self.check_use(place, false, state, range);
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
            projection: path
                .fields
                .iter()
                .map(|&f| ProjectionElem::Field(f))
                .collect(),
        };
        place_name(self.body, self.items, self.sigs, &place)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    maybe_uninit: Vec<bool>,
    maybe_moved: Vec<bool>,
    maybe_init: Vec<bool>,
}

impl State {
    /// Unions `other` into `self`; whether anything changed.
    fn join(&mut self, other: &State) -> bool {
        let mut changed = false;
        let mine = self
            .maybe_uninit
            .iter_mut()
            .chain(self.maybe_moved.iter_mut())
            .chain(self.maybe_init.iter_mut());
        let theirs = other
            .maybe_uninit
            .iter()
            .chain(&other.maybe_moved)
            .chain(&other.maybe_init);
        for (a, &b) in mine.zip(theirs) {
            if b && !*a {
                *a = true;
                changed = true;
            }
        }
        changed
    }
}
