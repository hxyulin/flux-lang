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
}

impl Input {
    pub fn from_lexed(lexed: &LexedStr<'_>) -> Input {
        let mut input = Input::default();
        let mut was_joint = false;
        for (i, &kind) in lexed.kinds().iter().enumerate() {
            if kind.is_trivia() {
                was_joint = false;
                continue;
            }
            if was_joint {
                input.set_last_joint();
            }
            input.push(kind);
            was_joint = i + 1 < lexed.len() && !lexed.kind(i + 1).is_trivia();
        }
        input
    }

    pub fn push(&mut self, kind: SyntaxKind) {
        let idx = self.kinds.len();
        if idx.is_multiple_of(64) {
            self.joint.push(0);
        }
        self.kinds.push(kind);
    }

    fn set_last_joint(&mut self) {
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

    /// Whether token `i` is immediately followed by token `i + 1`.
    pub fn is_joint(&self, i: usize) -> bool {
        i < self.kinds.len() && self.joint[i / 64] & (1 << (i % 64)) != 0
    }
}
