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

use rustc_hash::FxHashMap;
use std::hash::BuildHasher;

use flux_hir::{Diagnostic, ItemTree};
use flux_syntax::TextRange;
use flux_ty::Signatures;

use crate::bitset::BitSet;
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
        moved_uses: FxHashMap::default(),
        diagnostics: Vec::new(),
    };
    let mut state = State::new(paths.paths.len());
    for (block, data) in body.blocks.iter() {
        if !entry.load(block.index(), &mut state) {
            continue;
        }
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

/// The packed state of `paths` on entry to each block, recording which
/// blocks were reached. With `drop_moves`, a drop moves out of its place, as it does
/// for [`drops`](crate::drops); the check ignores drops, which run only
/// on what is initialized.
pub(crate) fn analyze(body: &Body, paths: &MovePaths, drop_moves: bool) -> EntryStates {
    let n = paths.paths.len();
    let mut entry = EntryStates::new(body.blocks.len(), n);
    let mut start = State::new(n);
    start.maybe_init = BitSet::new_filled(n);
    start.maybe_held = BitSet::new_filled(n);
    for (id, path) in paths.paths.iter().enumerate() {
        if !body.is_param(path.local) {
            start.maybe_uninit.insert(id);
            start.maybe_init.remove(id);
            start.maybe_held.remove(id);
        }
    }
    entry.join(0, &start);
    let mut state = start;
    let mut queued = BitSet::new_empty(body.blocks.len());
    queued.insert(0);
    let mut worklist = vec![Body::entry()];
    while let Some(block) = worklist.pop() {
        queued.remove(block.index());
        assert!(
            entry.load(block.index(), &mut state),
            "queued blocks are reached"
        );
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
            if entry.join(successor.index(), &state) && !queued.set(successor.index(), true) {
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

pub(crate) fn statement_events(kind: &StatementKind) -> impl Iterator<Item = Event<'_>> {
    let (rvalue, first, last) = match kind {
        StatementKind::Assign(dest, rvalue) => (
            Some(rvalue),
            match rvalue {
                Rvalue::Ref { place, .. } => Some(Event::Borrow(place)),
                Rvalue::Discriminant(place) => Some(Event::Read(place)),
                _ => None,
            },
            Some(Event::Assign(dest)),
        ),
        StatementKind::StorageDead(local) => (None, None, Some(Event::StorageDead(*local))),
        StatementKind::PlaceMention(_) => (None, None, None),
    };
    first
        .into_iter()
        .chain(
            rvalue
                .into_iter()
                .flat_map(Rvalue::operands)
                .filter_map(operand_event),
        )
        .chain(last)
}

pub(crate) fn terminator_events(kind: &TerminatorKind) -> impl Iterator<Item = Event<'_>> {
    let last = match kind {
        TerminatorKind::Call { dest, .. } | TerminatorKind::CallIndirect { dest, .. } => {
            Some(Event::Assign(dest))
        }
        TerminatorKind::Drop { place, .. } => Some(Event::Drop(place)),
        _ => None,
    };
    kind.operands().filter_map(operand_event).chain(last)
}

/// A local the analysis tracks, or a path of fields (and variants) into
/// one.
pub(crate) struct MovePath {
    pub(crate) local: Local,
    pub(crate) fields: crate::Projection,
    pub(crate) parent: Option<usize>,
    // One-based links keep optional child/sibling indices in four bytes.
    first_child: Option<std::num::NonZeroU32>,
    last_child: Option<std::num::NonZeroU32>,
    collision: Option<usize>,
    next_sibling: Option<std::num::NonZeroU32>,
}

pub(crate) struct MovePaths {
    pub(crate) paths: Vec<MovePath>,
    // Hash borrowed keys, then compare against the single owned copy in
    // `paths`. Collisions are chained through path indices.
    ids: FxHashMap<u64, usize>,
    /// Moves out of a struct with a `Drop` impl, E0509: they move
    /// nothing, as in rustc, so later uses are not errors too.
    forbidden: Vec<Place>,
}

/// The fields and variants of `place` before its first dereference or
/// index, and whether it has one: the elements of an array are not
/// tracked apart, as rustc's move paths stop at them (moving out of one
/// is E0508).
fn fields_before_deref(place: &Place) -> (&[ProjectionElem], bool) {
    match place
        .projection
        .iter()
        .position(|&e| matches!(e, ProjectionElem::Deref | ProjectionElem::Index(_)))
    {
        Some(deref) => (&place.projection[..deref], true),
        None => (&place.projection, false),
    }
}

impl MovePaths {
    /// The paths of `body`'s locals that `tracked` says, and of every
    /// place into them that it mentions.
    pub(crate) fn new(body: &Body, tracked: impl Fn(Local) -> bool) -> MovePaths {
        let mut paths = MovePaths {
            paths: Vec::new(),
            ids: FxHashMap::default(),
            forbidden: Vec::new(),
        };
        for (local, _) in body.locals.iter() {
            if tracked(local) {
                paths.insert(local, &[]);
            }
        }
        let mut add = |place: &Place| {
            if paths.lookup(place.local, &[]).is_some() {
                paths.insert(place.local, fields_before_deref(place).0);
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
        if let Some(id) = self.lookup(local, fields) {
            return id;
        }
        let parent = fields
            .split_last()
            .map(|(_, prefix)| self.insert(local, prefix));
        let id = self.paths.len();
        let hash = self.ids.hasher().hash_one((local, fields));
        let collision = self.ids.insert(hash, id);
        self.paths.push(MovePath {
            local,
            fields: fields.into(),
            parent,
            first_child: None,
            last_child: None,
            collision,
            next_sibling: None,
        });
        if let Some(parent) = parent {
            let link =
                std::num::NonZeroU32::new(u32::try_from(id + 1).expect("too many move paths"))
                    .expect("one-based path index");
            if let Some(last) = self.paths[parent].last_child {
                self.paths[(last.get() - 1) as usize].next_sibling = Some(link);
            }
            self.paths[parent].first_child.get_or_insert(link);
            self.paths[parent].last_child = Some(link);
        }
        id
    }

    fn lookup(&self, local: Local, fields: &[ProjectionElem]) -> Option<usize> {
        let hash = self.ids.hasher().hash_one((local, fields));
        let mut next = self.ids.get(&hash).copied();
        while let Some(id) = next {
            let path = &self.paths[id];
            if path.local == local && path.fields.as_ref() == fields {
                return Some(id);
            }
            next = path.collision;
        }
        None
    }

    /// The path of `place`, or of the pointer it is behind, and whether
    /// it is behind one.
    pub(crate) fn find(&self, place: &Place) -> Option<(usize, bool)> {
        let (fields, behind_deref) = fields_before_deref(place);
        let path = self.lookup(place.local, fields)?;
        Some((path, behind_deref))
    }

    /// The immediate children, in the order their paths were first mentioned.
    pub(crate) fn children(&self, path: usize) -> impl Iterator<Item = usize> + Clone + '_ {
        std::iter::successors(self.paths[path].first_child, |&p| {
            self.paths[(p.get() - 1) as usize].next_sibling
        })
        .map(|p| (p.get() - 1) as usize)
    }

    /// `path` and every path below it.
    pub(crate) fn subtree(&self, root: usize) -> impl Iterator<Item = usize> + Clone + '_ {
        let mut next = Some(root);
        std::iter::from_fn(move || {
            let path = next?;
            next = self.paths[path]
                .first_child
                .map(|p| (p.get() - 1) as usize)
                .or_else(|| {
                    let mut current = path;
                    while current != root {
                        if let Some(sibling) = self.paths[current].next_sibling {
                            return Some((sibling.get() - 1) as usize);
                        }
                        current = self.paths[current].parent.expect("descendant has a parent");
                    }
                    None
                });
            Some(path)
        })
    }

    /// The paths above `path`, nearest first.
    fn ancestors(&self, path: usize) -> impl Iterator<Item = usize> + Clone + '_ {
        std::iter::successors(self.paths[path].parent, |&p| self.paths[p].parent)
    }

    pub(crate) fn apply(&self, state: &mut State, event: &Event, drop_moves: bool) {
        let (place, effect): (Option<&Place>, fn(&mut State, usize)) = match event {
            Event::Move(place) if self.forbidden.contains(place) => return,
            Event::Move(place) | Event::Drop(place)
                if drop_moves || matches!(event, Event::Move(_)) =>
            {
                (Some(place), |state, p| {
                    state.maybe_uninit.insert(p);
                    state.maybe_moved.insert(p);
                    state.maybe_held.remove(p);
                })
            }
            Event::Drop(_) => return,
            Event::Move(_) => unreachable!("matched above"),
            Event::Assign(place) => (Some(place), |state, p| {
                state.maybe_uninit.remove(p);
                state.maybe_moved.remove(p);
                state.maybe_init.insert(p);
                state.maybe_held.insert(p);
            }),
            Event::StorageDead(local) => {
                if let Some(root) = self.lookup(*local, &[]) {
                    for p in self.subtree(root) {
                        state.maybe_uninit.insert(p);
                        state.maybe_moved.remove(p);
                        state.maybe_init.remove(p);
                        state.maybe_held.remove(p);
                    }
                }
                return;
            }
            Event::Read(_) | Event::Borrow(_) => return,
        };
        // Moving out of or assigning through a pointer leaves the pointer
        // as it was.
        if let Some((path, false)) = place.and_then(|place| self.find(place)) {
            for p in self.subtree(path) {
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
    moved_uses: FxHashMap<Vec<usize>, (usize, usize)>,
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
        // Through a pointer only the pointer itself is used.
        let below = self
            .paths
            .subtree(path)
            .take(if behind_deref { 1 } else { usize::MAX });
        let above = self.paths.ancestors(path);
        let used = if borrow { "borrow" } else { "use" };
        // The outermost moved path containing the place, if any.
        let moved = above
            .clone()
            .filter(|&p| state.maybe_moved[p])
            .last()
            .or_else(|| state.maybe_moved[path].then_some(path));
        let moves: Vec<usize> = match moved {
            Some(moved) => vec![moved],
            None => below.clone().filter(|&p| state.maybe_moved[p]).collect(),
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
                Some(moved) => format!("{used} of moved value{}", named(moved)),
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
        let message = if above.chain(below).any(|p| state.maybe_uninit[p]) {
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

/// A set of move paths each: `state.maybe_moved[path]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) maybe_uninit: BitSet,
    pub(crate) maybe_moved: BitSet,
    /// Ever assigned: what assigning an immutable local again needs.
    pub(crate) maybe_init: BitSet,
    /// Holding a value now: assigned, and not moved out of since.
    pub(crate) maybe_held: BitSet,
}

/// Four packed bitset rows per reached block, with no per-block allocations.
pub(crate) struct EntryStates {
    words: Vec<u64>,
    width: usize,
    rows: Vec<u32>,
}

impl EntryStates {
    fn new(blocks: usize, paths: usize) -> Self {
        let width = paths.div_ceil(64) * 4;
        Self {
            words: Vec::new(),
            width,
            rows: vec![u32::MAX; blocks],
        }
    }

    pub(crate) fn load(&self, block: usize, state: &mut State) -> bool {
        let row = self.rows[block];
        if row == u32::MAX {
            return false;
        }
        let start = row as usize * self.width;
        let row = &self.words[start..start + self.width];
        let mut offset = 0;
        for set in [
            &mut state.maybe_uninit,
            &mut state.maybe_moved,
            &mut state.maybe_init,
            &mut state.maybe_held,
        ] {
            let end = offset + set.words.len();
            set.words.copy_from_slice(&row[offset..end]);
            offset = end;
        }
        true
    }

    fn join(&mut self, block: usize, state: &State) -> bool {
        let mut changed = self.rows[block] == u32::MAX;
        if changed {
            self.rows[block] = u32::try_from(self.words.len().checked_div(self.width).unwrap_or(0))
                .expect("too many dataflow rows");
            self.words.resize(self.words.len() + self.width, 0);
        }
        let start = self.rows[block] as usize * self.width;
        let row = &mut self.words[start..start + self.width];
        let words = [
            &state.maybe_uninit,
            &state.maybe_moved,
            &state.maybe_init,
            &state.maybe_held,
        ]
        .into_iter()
        .flat_map(|set| &set.words);
        for (word, &other) in row.iter_mut().zip(words) {
            let new = *word | other;
            changed |= new != *word;
            *word = new;
        }
        changed
    }
}

impl State {
    pub(crate) fn new(paths: usize) -> Self {
        Self {
            maybe_uninit: BitSet::new_empty(paths),
            maybe_moved: BitSet::new_empty(paths),
            maybe_init: BitSet::new_empty(paths),
            maybe_held: BitSet::new_empty(paths),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreachable_blocks_do_not_reserve_bitset_rows() {
        let mut entries = EntryStates::new(100_000, 130);
        assert!(entries.words.is_empty());
        let state = State::new(130);
        entries.join(99_999, &state);
        assert_eq!(entries.words.len(), 12);
        let mut loaded = State::new(130);
        assert!(!entries.load(0, &mut loaded));
        assert!(entries.load(99_999, &mut loaded));
        assert_eq!(loaded, state);
    }

    #[test]
    fn packed_entries_preserve_each_analysis_and_reachability() {
        for paths in [0, 1, 63, 64, 65, 130] {
            let mut entries = EntryStates::new(3, paths);
            let mut state = State::new(paths);
            assert!(!entries.load(1, &mut state));
            assert!(entries.join(1, &state));
            assert!(!entries.join(1, &state));
            for p in 0..paths {
                state.maybe_uninit.set(p, p % 2 == 0);
                state.maybe_moved.set(p, p % 3 == 0);
                state.maybe_init.set(p, p % 5 == 0);
                state.maybe_held.set(p, p % 7 == 0);
            }
            entries.join(1, &state);
            let mut loaded = State::new(paths);
            assert!(entries.load(1, &mut loaded));
            assert_eq!(loaded, state);
            assert!(!entries.load(0, &mut loaded));
            assert!(entries.join(2, &State::new(paths)));
            assert!(entries.load(2, &mut loaded));
            assert_eq!(loaded, State::new(paths));
        }
    }

    #[test]
    fn borrowed_paths_and_subtrees_preserve_partial_moves() {
        let mut paths = MovePaths {
            paths: Vec::new(),
            ids: FxHashMap::default(),
            forbidden: Vec::new(),
        };
        let local = Local::from_raw(0);
        let field = ProjectionElem::Field(0);
        let nested = paths.insert(local, &[field, field]);
        let sibling = paths.insert(local, &[ProjectionElem::Field(1)]);
        let other = paths.insert(Local::from_raw(1), &[]);
        assert_eq!(
            paths.subtree(0).collect::<Vec<_>>(),
            [0, 1, nested, sibling]
        );
        assert_eq!(paths.subtree(1).collect::<Vec<_>>(), [1, nested]);
        assert_eq!(paths.children(0).collect::<Vec<_>>(), [1, sibling]);
        assert_eq!(paths.children(1).collect::<Vec<_>>(), [nested]);
        assert!(paths.children(other).next().is_none());
        assert_eq!(paths.insert(local, &[field, field]), nested);
        let place = Place {
            local,
            projection: vec![field, ProjectionElem::Deref, field].into(),
        };
        assert_eq!(paths.find(&place), Some((1, true)));
        let mut state = State::new(paths.paths.len());
        paths.apply(&mut state, &Event::Assign(&local.into()), false);
        paths.apply(
            &mut state,
            &Event::Move(&Place {
                local,
                projection: vec![field].into(),
            }),
            false,
        );
        assert!(state.maybe_moved[1] && state.maybe_moved[nested]);
        assert!(!state.maybe_moved[0] && !state.maybe_moved[sibling]);
        assert!(state.maybe_held[0] && state.maybe_held[sibling]);
        paths.apply(&mut state, &Event::StorageDead(local), false);
        assert!(paths.subtree(0).all(|p| state.maybe_uninit[p]
            && !state.maybe_init[p]
            && !state.maybe_held[p]
            && !state.maybe_moved[p]));
        assert!(!state.maybe_uninit[other]);
    }

    #[test]
    fn terminator_traversal_keeps_callee_order_and_duplicate_edges() {
        let local = |i| Place::from(Local::from_raw(i));
        let kind = TerminatorKind::CallIndirect {
            callee: Operand::Copy(local(0)),
            args: vec![Operand::Move(local(1)), Operand::Copy(local(2))],
            dest: local(3),
            target: None,
        };
        assert!(kind.successors().next().is_none());
        let mut events = terminator_events(&kind);
        assert!(matches!(events.next(), Some(Event::Read(p)) if p == &local(0)));
        assert!(matches!(events.next(), Some(Event::Move(p)) if p == &local(1)));
        assert!(matches!(events.next(), Some(Event::Read(p)) if p == &local(2)));
        assert!(matches!(events.next(), Some(Event::Assign(p)) if p == &local(3)));
        assert!(events.next().is_none());
        let block = crate::BasicBlock::from_raw;
        let switch = TerminatorKind::SwitchInt {
            discr: Operand::Copy(local(0)),
            targets: vec![(0, block(2)), (1, block(2)), (2, block(1))],
            otherwise: block(3),
            variants: None,
        };
        assert_eq!(
            switch.successors().collect::<Vec<_>>(),
            [block(2), block(2), block(1), block(3)]
        );
    }

    #[test]
    fn assignment_events_read_operands_before_writing_destination() {
        let dest = Place::from(Local::from_raw(0));
        let source = Place::from(Local::from_raw(1));
        let kind = StatementKind::Assign(
            dest.clone(),
            Rvalue::Aggregate(
                crate::AggregateKind::Tuple,
                vec![Operand::Copy(source.clone()), Operand::Move(dest.clone())],
            ),
        );
        let mut events = statement_events(&kind);
        assert!(matches!(events.next(), Some(Event::Read(p)) if p == &source));
        assert!(matches!(events.next(), Some(Event::Move(p)) if p == &dest));
        assert!(matches!(events.next(), Some(Event::Assign(p)) if p == &dest));
        assert!(events.next().is_none());
    }
}
