//! The parser's view of a token stream: significant tokens only, each with
//! a flag saying whether it touches the next one.

use crate::SyntaxKind;
use crate::lexer::LexedStr;

/// Non-trivia token kinds, plus a "joint" bit per token. Token `i` is joint
/// when token `i + 1` follows it with no trivia in between, which is how
/// the parser tells `>>` from `> >`.
#[derive(Debug, Default)]
pub struct Input {
    kinds: Vec<SyntaxKind>,
    joint: Vec<u64>,
    /// Identifiers spelled `union`, excluding raw identifiers.
    union_keywords: Vec<u64>,
}

impl Input {
    pub fn from_lexed(lexed: &LexedStr<'_>) -> Input {
        let mut input = Input {
            kinds: Vec::with_capacity(lexed.len()),
            joint: Vec::with_capacity(lexed.len() / 64 + 1),
            union_keywords: Vec::with_capacity(lexed.len() / 64 + 1),
        };
        let mut was_joint = false;
        for (i, &kind) in lexed.kinds().iter().enumerate() {
            if kind.is_trivia() {
                was_joint = false;
                continue;
            }
            if was_joint {
                input.set_last_joint();
            }
            input.push(kind, lexed.is_union_keyword(i));
            was_joint = i + 1 < lexed.len() && !lexed.kind(i + 1).is_trivia();
        }
        input
    }

    /// Adds a token and its lexer-supplied contextual classification. The
    /// grammar still sees `IDENT` except where it requests a keyword.
    pub fn push(&mut self, kind: SyntaxKind, is_union: bool) {
        let idx = self.kinds.len();
        if idx.is_multiple_of(64) {
            self.joint.push(0);
            self.union_keywords.push(0);
        }
        if kind == SyntaxKind::IDENT && is_union {
            self.union_keywords[idx / 64] |= 1 << (idx % 64);
        }
        self.kinds.push(kind);
    }

    /// Marks the last token pushed as joint with the next.
    pub fn set_last_joint(&mut self) {
        let idx = self.kinds.len() - 1;
        self.joint[idx / 64] |= 1 << (idx % 64);
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// The kind of token `i`, or `EOF` past the end.
    pub fn kind(&self, i: usize) -> SyntaxKind {
        self.kinds.get(i).copied().unwrap_or(SyntaxKind::EOF)
    }

    pub(crate) fn is_union_keyword(&self, i: usize) -> bool {
        self.union_keywords
            .get(i / 64)
            .is_some_and(|word| word & (1 << (i % 64)) != 0)
    }

    /// Whether token `i` is immediately followed by token `i + 1`.
    pub fn is_joint(&self, i: usize) -> bool {
        i < self.kinds.len() && self.joint[i / 64] & (1 << (i % 64)) != 0
    }
}
