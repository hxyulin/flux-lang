//! The parser's output: a flat list of events describing the tree.
//!
//! Emitting events instead of building the tree directly keeps the parser
//! independent of the tree representation and of the source text, and lets
//! a finished node be wrapped later (`1 + 2` becomes a binary expression
//! only after the `+` is seen) without moving anything.
//!
//! A file produces about two events per token, so `Event` is kept to 8
//! bytes; error messages live in a side table.

use std::num::NonZeroU32;

use crate::SyntaxKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Starts a node. `forward_parent`, if set, is the offset (from this
    /// event) of a `Start` for a node that must be opened *before* this
    /// one: it was created by `CompletedMarker::precede` to wrap this node.
    Start {
        kind: SyntaxKind,
        forward_parent: Option<NonZeroU32>,
    },
    /// Finishes the most recently started node.
    Finish,
    /// Consumes `n_raw_tokens` significant tokens as a single token of
    /// `kind`. Composite punctuation (`::`) spans several raw tokens.
    Token { kind: SyntaxKind, n_raw_tokens: u8 },
    /// An error at the current position; `index` is into
    /// [`Output::errors`].
    Error { index: u32 },
}

const _: () = assert!(std::mem::size_of::<Event>() == 8);

impl Event {
    pub(crate) fn tombstone() -> Event {
        Event::Start {
            kind: SyntaxKind::TOMBSTONE,
            forward_parent: None,
        }
    }
}

/// Everything the parser produces for one input.
#[derive(Debug, Clone, Default)]
pub struct Output {
    pub events: Vec<Event>,
    pub errors: Vec<String>,
}
