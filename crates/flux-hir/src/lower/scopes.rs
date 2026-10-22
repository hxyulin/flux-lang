//! The local bindings in scope while a body is lowered.

use flux_intern::Symbol;
use rustc_hash::FxHashMap;

use crate::body::BindingId;

/// Bindings in scope, innermost last, as a stack that a block truncates
/// on exit. Each name maps to its innermost binding, and each binding
/// keeps the one it shadows, so looking a name up takes constant time
/// however many bindings are in scope.
#[derive(Default)]
pub(super) struct Scopes {
    /// Each binding, with the index of the one of the same name it
    /// shadows.
    stack: Vec<(Symbol, BindingId, Option<u32>)>,
    /// The index in `stack` of the innermost binding of each name.
    innermost: FxHashMap<Symbol, u32>,
}

impl Scopes {
    pub(super) fn new() -> Scopes {
        Scopes::default()
    }

    /// Brings `binding` into scope as `name`, shadowing any before it.
    pub(super) fn push(&mut self, (name, binding): (Symbol, BindingId)) {
        let index = u32::try_from(self.stack.len()).expect("fewer than 2^32 bindings");
        let shadowed = self.innermost.insert(name, index);
        self.stack.push((name, binding, shadowed));
    }

    /// The number of bindings in scope, to [`Scopes::truncate`] to later.
    pub(super) fn len(&self) -> usize {
        self.stack.len()
    }

    /// Ends the scope of every binding after the first `len`.
    pub(super) fn truncate(&mut self, len: usize) {
        while self.stack.len() > len {
            let (name, _, shadowed) = self.stack.pop().expect("longer than `len`");
            match shadowed {
                Some(index) => self.innermost.insert(name, index),
                None => self.innermost.remove(&name),
            };
        }
    }

    /// The innermost binding named `name`.
    pub(super) fn get(&self, name: Symbol) -> Option<BindingId> {
        self.innermost
            .get(&name)
            .map(|&index| self.stack[index as usize].1)
    }
}
