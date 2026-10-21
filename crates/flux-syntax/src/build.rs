//! Turns parser events and lexer tokens into a [`SyntaxTree`].
//!
//! The parser never sees trivia, so this is where whitespace and comments
//! are placed: before the next token, inside the innermost open node, except
//! that comments directly above an item belong to the item. The result
//! contains every byte of the source.
//!
//! The tree reuses the lexer's token arrays. Composite tokens (`::` from two
//! `:`) shrink the token count, so tokens are compacted in place as they are
//! placed: the write position never passes the read position.

use crate::lexer::LexedStr;
use crate::parser::{Event, Output};
use crate::tree::{NodeData, SyntaxTree};
use crate::{SyntaxError, SyntaxKind, SyntaxKind::*, TextRange};
use std::mem;

/// Builds the tree for `output`, which the parser produced from `lexed`.
pub fn build_tree(lexed: LexedStr<'_>, output: Output) -> (SyntaxTree, Vec<SyntaxError>) {
    let Output {
        mut events,
        errors: mut messages,
    } = output;
    let text = lexed.text();
    // Lexer errors name raw token indices, which compaction invalidates.
    let mut errors: Vec<SyntaxError> = lexed
        .errors()
        .iter()
        .map(|e| SyntaxError {
            message: e.message.clone(),
            range: lexed.token_range(e.token),
        })
        .collect();
    let (kinds, starts, _) = lexed.into_parts();
    let mut builder = Builder {
        text,
        len: kinds.len(),
        kinds,
        starts,
        read: 0,
        write: 0,
        // Nodes average about one per two tokens; events are about two
        // per token.
        nodes: Vec::with_capacity(events.len() / 4 + 1),
        open: Vec::new(),
        errors: &mut errors,
    };

    let mut forward_parents = Vec::new();
    for i in 0..events.len() {
        match mem::replace(&mut events[i], Event::tombstone()) {
            Event::Start {
                kind,
                forward_parent,
            } => {
                // A node wrapped by `precede` starts with a chain of
                // forward parents: open the outermost first.
                forward_parents.push(kind);
                let mut idx = i;
                let mut next = forward_parent;
                while let Some(offset) = next {
                    idx += offset.get() as usize;
                    next = match mem::replace(&mut events[idx], Event::tombstone()) {
                        Event::Start {
                            kind,
                            forward_parent,
                        } => {
                            forward_parents.push(kind);
                            forward_parent
                        }
                        _ => unreachable!("forward parent is not a Start event"),
                    };
                }
                for kind in forward_parents.drain(..).rev() {
                    if kind != TOMBSTONE {
                        builder.start_node(kind);
                    }
                }
            }
            Event::Finish => builder.finish_node(),
            Event::Token { kind, n_raw_tokens } => builder.token(kind, n_raw_tokens as usize),
            Event::Error { index } => {
                builder.error(mem::take(&mut messages[index as usize]));
            }
        }
    }
    let tree = builder.finish();
    errors.sort_by_key(|e| (e.range.start(), e.range.end()));
    (tree, errors)
}

struct Builder<'a, 'e> {
    text: &'a str,
    /// Number of lexer tokens.
    len: usize,
    /// The lexer's arrays; `..write` holds the tree's tokens so far and
    /// `read..` the lexer tokens not yet placed.
    kinds: Vec<SyntaxKind>,
    starts: Vec<u32>,
    read: usize,
    write: usize,
    nodes: Vec<NodeData>,
    /// Indices of the nodes not yet finished.
    open: Vec<u32>,
    errors: &'e mut Vec<SyntaxError>,
}

impl Builder<'_, '_> {
    fn start_node(&mut self, kind: SyntaxKind) {
        // The root starts before any leading trivia.
        if !self.nodes.is_empty() {
            let n_trivia = self.kinds[self.read..self.len]
                .iter()
                .take_while(|kind| kind.is_trivia())
                .count();
            let n_attached = if kind.takes_leading_comments() {
                self.n_attached_trivia(n_trivia)
            } else {
                0
            };
            self.eat_n_trivia(n_trivia - n_attached);
        }
        self.open.push(self.nodes.len() as u32);
        self.nodes.push(NodeData {
            kind,
            first_token: self.write as u32,
            end_token: 0,
            descendants: 0,
        });
    }

    fn finish_node(&mut self) {
        let index = self.open.pop().expect("Finish without a Start") as usize;
        // Trailing trivia stays outside every node but the root.
        if self.open.is_empty() {
            self.eat_trivia();
        }
        let descendants = (self.nodes.len() - index - 1) as u32;
        let node = &mut self.nodes[index];
        node.end_token = self.write as u32;
        node.descendants = descendants;
    }

    fn token(&mut self, kind: SyntaxKind, n_raw_tokens: usize) {
        self.eat_trivia();
        // A composite token spans several joint lexer tokens.
        self.kinds[self.write] = kind;
        self.starts[self.write] = self.starts[self.read];
        self.write += 1;
        self.read += n_raw_tokens;
    }

    fn error(&mut self, message: String) {
        let offset = self.starts[self.read];
        self.errors.push(SyntaxError {
            message,
            range: TextRange::empty(offset),
        });
    }

    fn finish(mut self) -> SyntaxTree {
        assert!(self.open.is_empty(), "unbalanced events");
        assert_eq!(self.read, self.len, "not every token was placed");
        self.starts[self.write] = self.text.len() as u32;
        self.kinds.truncate(self.write);
        self.starts.truncate(self.write + 1);
        // Trees outlive parsing, often many at once; the capacity estimates
        // above leave about a third of the memory unused.
        self.kinds.shrink_to_fit();
        self.starts.shrink_to_fit();
        self.nodes.shrink_to_fit();
        SyntaxTree::new(self.text.into(), self.kinds, self.starts, self.nodes)
    }

    fn eat_trivia(&mut self) {
        while self.read < self.len && self.kinds[self.read].is_trivia() {
            self.bump_raw();
        }
    }

    fn eat_n_trivia(&mut self, n: usize) {
        for _ in 0..n {
            self.bump_raw();
        }
    }

    fn bump_raw(&mut self) {
        self.kinds[self.write] = self.kinds[self.read];
        self.starts[self.write] = self.starts[self.read];
        self.write += 1;
        self.read += 1;
    }

    /// How many of the `n_trivia` tokens ahead belong to the item that
    /// starts after them: its comments, unless a blank line or an inner
    /// doc comment (`//!`) separates them.
    fn n_attached_trivia(&self, n_trivia: usize) -> usize {
        let mut attached = 0;
        for back in 0..n_trivia {
            let i = self.read + n_trivia - 1 - back;
            let text = &self.text[self.starts[i] as usize..self.starts[i + 1] as usize];
            match self.kinds[i] {
                WHITESPACE if text.contains("\n\n") || text.contains("\n\r\n") => break,
                COMMENT if text.starts_with("//!") || text.starts_with("/*!") => break,
                COMMENT => attached = back + 1,
                _ => {}
            }
        }
        attached
    }
}
