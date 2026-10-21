//! Definite initialization and moves: every place is assigned, and not
//! moved out of, before it is read, and an immutable local is assigned at
//! most once.
//!
//! The analysis tracks *move paths*: each user local and parameter, and
//! every field path into one that the body mentions (`s`, `s.inner`,
//! `s.inner.x`). Three forward dataflow analyses over the control-flow
//! graph, all joined by union, give for each path whether it *may be
//! uninitialized* (never assigned, or moved out of), *may be moved* (to
//! tell a use after move, E0382, from a use of an uninitialized binding,
//! E0381) and *may be initialized* (assigning an immutable local again is
//! E0384). Moving or assigning a path does the same to every path below
//! it. Temporaries are initialized by construction, and parameters on
//! entry. Unreachable blocks are never analyzed, as in rustc.

use std::collections::HashMap;

use flux_hir::{Diagnostic, ItemTree};
use flux_syntax::TextRange;
use flux_ty::{Signatures, TyKind};

use crate::{Body, Local, LocalKind, Operand, Place, Rvalue, StatementKind, TerminatorKind};

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
        if !paths.is_param(body, path.local) {
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
            let StatementKind::Assign(dest, rvalue) = &statement.kind;
            for operand in rvalue_operands(rvalue) {
                paths.effect_of_use(&mut state, operand);
            }
            paths.effect_of_assign(&mut state, dest);
        }
        for operand in terminator_operands(&data.terminator.kind) {
            paths.effect_of_use(&mut state, operand);
        }
        if let TerminatorKind::Call { dest, .. } = &data.terminator.kind {
            paths.effect_of_assign(&mut state, dest);
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
            let StatementKind::Assign(dest, rvalue) = &statement.kind;
            for operand in rvalue_operands(rvalue) {
                report.check_use(operand, &state, statement.range);
                paths.effect_of_use(&mut state, operand);
            }
            report.check_assign(dest, &state, statement.range);
            paths.effect_of_assign(&mut state, dest);
        }
        let range = data.terminator.range;
        for operand in terminator_operands(&data.terminator.kind) {
            report.check_use(operand, &state, range);
            paths.effect_of_use(&mut state, operand);
        }
    }
    report.diagnostics
}

fn rvalue_operands(rvalue: &Rvalue) -> Vec<&Operand> {
    match rvalue {
        Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(op, _) => vec![op],
        Rvalue::BinaryOp(_, l, r) => vec![l, r],
        Rvalue::Aggregate(_, ops) => ops.iter().collect(),
    }
}

fn terminator_operands(kind: &TerminatorKind) -> Vec<&Operand> {
    match kind {
        TerminatorKind::SwitchInt { discr, .. } => vec![discr],
        TerminatorKind::Call { args, .. } => args.iter().collect(),
        TerminatorKind::Assert { cond, .. } => vec![cond],
        TerminatorKind::Goto(_) | TerminatorKind::Return | TerminatorKind::Unreachable => {
            Vec::new()
        }
    }
}

/// A user local or parameter, or a path of fields into one.
struct MovePath {
    local: Local,
    projection: Box<[u32]>,
    parent: Option<usize>,
    children: Vec<usize>,
}

struct MovePaths {
    paths: Vec<MovePath>,
    ids: HashMap<(Local, Box<[u32]>), usize>,
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
            if paths.is_param(body, local) || matches!(decl.kind, LocalKind::User { .. }) {
                paths.insert(local, &[]);
            }
        }
        let mut add = |place: &Place| {
            if paths.ids.contains_key(&(place.local, Box::default())) {
                paths.insert(place.local, &place.projection);
            }
        };
        let add_operand = |add: &mut dyn FnMut(&Place), op: &Operand| {
            if let Operand::Copy(place) | Operand::Move(place) = op {
                add(place);
            }
        };
        for data in body.blocks.values() {
            for statement in &data.statements {
                let StatementKind::Assign(dest, rvalue) = &statement.kind;
                add(dest);
                for op in rvalue_operands(rvalue) {
                    add_operand(&mut add, op);
                }
            }
            for op in terminator_operands(&data.terminator.kind) {
                add_operand(&mut add, op);
            }
            if let TerminatorKind::Call { dest, .. } = &data.terminator.kind {
                add(dest);
            }
        }
        paths
    }

    fn is_param(&self, body: &Body, local: Local) -> bool {
        (1..=body.param_count).contains(&local.index())
    }

    /// The path for `local` with `projection` and its prefixes.
    fn insert(&mut self, local: Local, projection: &[u32]) -> usize {
        if let Some(&id) = self.ids.get(&(local, projection.into())) {
            return id;
        }
        let parent = projection
            .split_last()
            .map(|(_, prefix)| self.insert(local, prefix));
        let id = self.paths.len();
        self.paths.push(MovePath {
            local,
            projection: projection.into(),
            parent,
            children: Vec::new(),
        });
        if let Some(parent) = parent {
            self.paths[parent].children.push(id);
        }
        self.ids.insert((local, projection.into()), id);
        id
    }

    fn find(&self, place: &Place) -> Option<usize> {
        self.ids
            .get(&(place.local, place.projection.clone()))
            .copied()
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

    fn effect_of_use(&self, state: &mut State, operand: &Operand) {
        if let Operand::Move(place) = operand
            && let Some(path) = self.find(place)
        {
            let mut subtree = Vec::new();
            self.subtree(path, &mut subtree);
            for p in subtree {
                state.maybe_uninit[p] = true;
                state.maybe_moved[p] = true;
            }
        }
    }

    fn effect_of_assign(&self, state: &mut State, place: &Place) {
        if let Some(path) = self.find(place) {
            let mut subtree = Vec::new();
            self.subtree(path, &mut subtree);
            for p in subtree {
                state.maybe_uninit[p] = false;
                state.maybe_moved[p] = false;
                state.maybe_init[p] = true;
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
        self.diagnostics.push(Diagnostic { range, message });
    }

    fn check_use(&mut self, operand: &Operand, state: &State, range: TextRange) {
        let (Operand::Copy(place) | Operand::Move(place)) = operand else {
            return;
        };
        let Some(path) = self.paths.find(place) else {
            return;
        };
        if self.reported[place.local.index()] {
            return;
        }
        let mut below = Vec::new();
        self.paths.subtree(path, &mut below);
        let above: Vec<usize> = self.paths.ancestors(path).collect();
        // The outermost moved path containing the place, if any.
        let moved = above
            .iter()
            .rev()
            .chain([&path])
            .find(|&&p| state.maybe_moved[p]);
        let message = if let Some(&moved) = moved {
            let name = self.path_name(moved);
            format!("use of moved value: `{name}`")
        } else if below.iter().any(|&p| state.maybe_moved[p]) {
            format!("use of partially moved value: `{}`", self.path_name(path))
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
        let Some(path) = self.paths.find(place) else {
            return;
        };
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
            let place = self.path_name(path);
            format!("cannot assign to `{place}`, as `{name}` is not declared as mutable")
        } else {
            return;
        };
        self.error(range, message);
    }

    fn local_name(&self, local: Local) -> String {
        match self.body.locals[local].kind {
            LocalKind::User { name, .. } => name.to_string(),
            _ => format!("_{}", local.index()),
        }
    }

    /// `s.inner.0`, as rustc writes places.
    fn path_name(&self, path: usize) -> String {
        let path = &self.paths.paths[path];
        let mut name = self.local_name(path.local);
        let mut ty = self.body.locals[path.local].ty;
        for &field in &path.projection {
            match ty.kind() {
                TyKind::Adt(adt) => {
                    let field_name = self.items.structs[adt.strukt].fields[field as usize].name;
                    name.push('.');
                    name.push_str(field_name.as_str());
                }
                _ => name.push_str(&format!(".{field}")),
            }
            ty = self
                .sigs
                .fields(ty)
                .map_or(flux_ty::Ty::ERROR, |fields| fields[field as usize]);
        }
        name
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
