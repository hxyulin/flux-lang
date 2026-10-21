//! Definite initialization: every local is assigned before it is read,
//! and an immutable local at most once.
//!
//! Two forward dataflow analyses over the control-flow graph, both joined
//! by union: which user locals *may be uninitialized* (a read of one is
//! E0381), and which *may be initialized* (assigning an immutable one
//! again is E0384). Temporaries are initialized by construction, and
//! parameters on entry. Unreachable blocks are never analyzed, as in
//! rustc.

use flux_hir::Diagnostic;

use crate::{Body, Local, LocalKind, Operand, Rvalue, StatementKind, TerminatorKind};

/// The diagnostics for `body`'s uses of uninitialized locals and repeated
/// assignments of immutable ones.
pub fn check(body: &Body) -> Vec<Diagnostic> {
    let n = body.locals.len();
    let is_user = |local: Local| matches!(body.locals[local].kind, LocalKind::User { .. });

    // State on entry to each block, `None` until reached.
    let mut entry: Vec<Option<State>> = vec![None; body.blocks.len()];
    entry[0] = Some(State {
        maybe_uninit: (0..n).map(|i| is_user(Local::from_raw(i as u32))).collect(),
        maybe_init: vec![false; n],
    });
    let mut worklist = vec![Body::entry()];
    while let Some(block) = worklist.pop() {
        let mut state = entry[block.index()]
            .clone()
            .expect("queued blocks are reached");
        let data = &body.blocks[block];
        for statement in &data.statements {
            let StatementKind::Assign(dest, _) = statement.kind;
            state.assign(dest);
        }
        let call_dest = match &data.terminator.kind {
            TerminatorKind::Call { dest, .. } => Some(*dest),
            _ => None,
        };
        if let Some(dest) = call_dest {
            state.assign(dest);
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
    let mut diagnostics = Vec::new();
    let mut reassigned = Vec::new();
    let mut reported_uses = vec![false; n];
    for (block, data) in body.blocks.iter() {
        let Some(mut state) = entry[block.index()].clone() else {
            continue;
        };
        let mut uses = Uses {
            body,
            reported: &mut reported_uses,
            diagnostics: &mut diagnostics,
        };
        let mut use_of =
            |operand: &Operand, state: &State, range| uses.check(operand, state, range);
        for statement in &data.statements {
            let StatementKind::Assign(dest, rvalue) = &statement.kind;
            match rvalue {
                Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(op, _) => {
                    use_of(op, &state, statement.range);
                }
                Rvalue::BinaryOp(_, l, r) => {
                    use_of(l, &state, statement.range);
                    use_of(r, &state, statement.range);
                }
            }
            if let LocalKind::User {
                name,
                mutable: false,
            } = body.locals[*dest].kind
                && state.maybe_init[dest.index()]
            {
                reassigned.push(Diagnostic {
                    range: statement.range,
                    message: format!("cannot assign twice to immutable variable `{name}`"),
                });
            }
            state.assign(*dest);
        }
        let range = data.terminator.range;
        match &data.terminator.kind {
            TerminatorKind::SwitchInt { discr, .. } => use_of(discr, &state, range),
            TerminatorKind::Call { args, .. } => {
                for arg in args {
                    use_of(arg, &state, range);
                }
            }
            TerminatorKind::Assert { cond, .. } => use_of(cond, &state, range),
            TerminatorKind::Goto(_) | TerminatorKind::Return | TerminatorKind::Unreachable => {}
        }
    }
    diagnostics.extend(reassigned);
    diagnostics
}

/// Reports reads of locals that may be uninitialized, once per local.
struct Uses<'a> {
    body: &'a Body,
    reported: &'a mut Vec<bool>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Uses<'_> {
    fn check(&mut self, operand: &Operand, state: &State, range: flux_syntax::TextRange) {
        let Operand::Copy(local) = operand else {
            return;
        };
        if !state.maybe_uninit[local.index()] || self.reported[local.index()] {
            return;
        }
        self.reported[local.index()] = true;
        let LocalKind::User { name, .. } = self.body.locals[*local].kind else {
            unreachable!("only user locals start uninitialized")
        };
        let how = if state.maybe_init[local.index()] {
            "is possibly-uninitialized"
        } else {
            "isn't initialized"
        };
        self.diagnostics.push(Diagnostic {
            range,
            message: format!("used binding `{name}` {how}"),
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    maybe_uninit: Vec<bool>,
    maybe_init: Vec<bool>,
}

impl State {
    fn assign(&mut self, local: Local) {
        self.maybe_uninit[local.index()] = false;
        self.maybe_init[local.index()] = true;
    }

    /// Unions `other` into `self`; whether anything changed.
    fn join(&mut self, other: &State) -> bool {
        let mut changed = false;
        for (a, b) in self
            .maybe_uninit
            .iter_mut()
            .chain(self.maybe_init.iter_mut())
            .zip(other.maybe_uninit.iter().chain(&other.maybe_init))
        {
            if *b && !*a {
                *a = true;
                changed = true;
            }
        }
        changed
    }
}
