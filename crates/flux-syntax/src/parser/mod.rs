//! A hand-written recursive-descent parser with a Pratt loop for
//! expressions.
//!
//! The parser reads only token kinds ([`Input`]) and writes only
//! [`Event`]s. It never sees source text or tree nodes, so it can run on any
//! thread, and it never fails: missing tokens produce error events and
//! unexpected tokens are wrapped in `ERROR` nodes.

mod event;
mod grammar;
mod input;

pub use event::{Event, Output};
pub use input::Input;

use std::num::NonZeroU32;

use crate::SyntaxKind::{self, *};
use crate::T;

/// Parses a whole source file into events.
pub fn parse_source_file(input: &Input) -> Output {
    let mut p = Parser::new(input);
    grammar::source_file(&mut p);
    p.output
}

/// A kind of syntax a macro's matcher can name (`$e:expr`), or that a
/// macro call's expansion is parsed as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fragment {
    Expr,
    /// A pattern, `|` included.
    Pat,
    /// A pattern without a top-level `|`.
    PatParam,
    Type,
    /// A path, as in a type: `a::B<T>`.
    Path,
    Block,
    /// A statement without its trailing `;`.
    Stmt,
    Item,
    Vis,
    Lifetime,
    Literal,
    /// An attribute's contents.
    Meta,
    /// Items until the end, as a module's: a macro call in item position.
    Items,
    /// Statements until the end, the last maybe an expression without
    /// `;`, as a block's: a macro call in statement position.
    Stmts,
}

/// Parses the whole of `input` as a macro expansion: one root node,
/// `MACRO_ITEMS` or `MACRO_STMTS` for those fragments and
/// `MACRO_FRAGMENT` around the fragment otherwise. Tokens left after a
/// fragment are wrapped in an `ERROR` node with the error
/// [`EXPANSION_IGNORES`].
pub fn parse_expansion(input: &Input, fragment: Fragment) -> Output {
    let mut p = Parser::new(input);
    p.eof_ends_block = true;
    grammar::expansion(&mut p, fragment);
    p.output
}

/// The error a fragment with tokens after it has, at the first of them.
pub const EXPANSION_IGNORES: &str = "macro expansion ignores the tokens after the fragment";

/// How many raw tokens of `input`, from `start` on, a fragment of kind
/// `fragment` takes: `None` if they do not make one without errors.
pub fn fragment_len(input: &Input, start: usize, fragment: Fragment) -> Option<usize> {
    fragment_or_error(input, start, fragment).ok()
}

/// Like [`fragment_len`], with the token where the parser found an error
/// if there was one.
pub fn fragment_or_error(input: &Input, start: usize, fragment: Fragment) -> Result<usize, usize> {
    let mut p = Parser::starting_at(input, start);
    grammar::fragment(&mut p, fragment);
    let mut pos = start;
    for e in &p.output.events {
        match *e {
            Event::Token { n_raw_tokens, .. } => pos += n_raw_tokens as usize,
            Event::Error { .. } => return Err(pos),
            _ => {}
        }
    }
    Ok(p.pos - start)
}

/// A set of token kinds, for recovery and lookahead.
#[derive(Clone, Copy)]
pub(crate) struct TokenSet([u64; 3]);

impl TokenSet {
    pub(crate) const EMPTY: TokenSet = TokenSet([0; 3]);

    pub(crate) const fn new(kinds: &[SyntaxKind]) -> TokenSet {
        let mut bits = [0u64; 3];
        let mut i = 0;
        while i < kinds.len() {
            let k = kinds[i] as usize;
            bits[k / 64] |= 1 << (k % 64);
            i += 1;
        }
        TokenSet(bits)
    }

    pub(crate) const fn union(self, other: TokenSet) -> TokenSet {
        TokenSet([
            self.0[0] | other.0[0],
            self.0[1] | other.0[1],
            self.0[2] | other.0[2],
        ])
    }

    pub(crate) const fn contains(&self, kind: SyntaxKind) -> bool {
        let k = kind as usize;
        k < 192 && self.0[k / 64] & (1 << (k % 64)) != 0
    }
}

const _: () = assert!(
    (SyntaxKind::ERROR as usize) < 192,
    "TokenSet holds token kinds only; tokens must stay below 192"
);

pub(crate) struct Parser<'i> {
    input: &'i Input,
    pos: usize,
    output: Output,
    /// Guards against grammar bugs that loop without consuming input.
    steps: u32,
    /// The end of the input closes a block, as at the end of statements
    /// a macro expanded to.
    eof_ends_block: bool,
}

const STEP_LIMIT: u32 = 15_000_000;

impl<'i> Parser<'i> {
    fn new(input: &'i Input) -> Parser<'i> {
        Parser {
            input,
            pos: 0,
            output: Output {
                // About two events per token.
                events: Vec::with_capacity(input.len() * 2 + 2),
                errors: Vec::new(),
            },
            steps: 0,
            eof_ends_block: false,
        }
    }

    /// A parser for a fragment starting at token `start`, which does not
    /// expect to read the rest of the input.
    fn starting_at(input: &'i Input, start: usize) -> Parser<'i> {
        Parser {
            input,
            pos: start,
            output: Output {
                events: Vec::with_capacity(16),
                errors: Vec::new(),
            },
            steps: 0,
            eof_ends_block: false,
        }
    }

    /// At the `}` that ends a block, or the end of statements a macro
    /// expanded to.
    pub(crate) fn at_block_end(&mut self) -> bool {
        self.at(T!['}']) || (self.eof_ends_block && self.at_end())
    }

    // The lookahead functions below are inlined: grammar rules call them
    // with constant kinds, for which the test for composite tokens folds
    // away.

    /// The kind of the `n`th token ahead, as raw (single-character) tokens.
    #[inline]
    pub(crate) fn nth(&mut self, n: usize) -> SyntaxKind {
        self.steps += 1;
        assert!(self.steps < STEP_LIMIT, "the parser seems stuck");
        self.input.kind(self.pos + n)
    }

    #[inline]
    pub(crate) fn current(&mut self) -> SyntaxKind {
        self.nth(0)
    }

    /// Whether the next token is `kind`. Composite kinds (`::`) match when
    /// their parts are present and joint.
    #[inline]
    pub(crate) fn at(&mut self, kind: SyntaxKind) -> bool {
        self.nth_at(0, kind)
    }

    #[inline]
    pub(crate) fn nth_at(&mut self, n: usize, kind: SyntaxKind) -> bool {
        match kind.composite_parts() {
            None => self.nth(n) == kind,
            Some(parts) => self.nth_at_composite(n, parts),
        }
    }

    fn nth_at_composite(&self, n: usize, parts: &[SyntaxKind]) -> bool {
        let start = self.pos + n;
        parts.iter().enumerate().all(|(i, &part)| {
            self.input.kind(start + i) == part
                && (i + 1 == parts.len() || self.input.is_joint(start + i))
        })
    }

    #[inline]
    pub(crate) fn at_ts(&mut self, set: TokenSet) -> bool {
        set.contains(self.current())
    }

    #[inline]
    pub(crate) fn at_end(&mut self) -> bool {
        self.at(EOF)
    }

    /// Consumes the next token if it is `kind`.
    #[inline]
    pub(crate) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if !self.at(kind) {
            return false;
        }
        let n_raw = kind.composite_parts().map_or(1, |parts| parts.len());
        self.do_bump(kind, n_raw as u8);
        true
    }

    /// Consumes the next token, which must be `kind`.
    #[track_caller]
    pub(crate) fn bump(&mut self, kind: SyntaxKind) {
        assert!(
            self.eat(kind),
            "expected {kind:?}, found {:?}",
            self.current()
        );
    }

    /// Consumes the next raw token as-is.
    pub(crate) fn bump_any(&mut self) {
        let kind = self.current();
        if kind != EOF {
            self.do_bump(kind, 1);
        }
    }

    fn do_bump(&mut self, kind: SyntaxKind, n_raw_tokens: u8) {
        self.pos += n_raw_tokens as usize;
        self.steps = 0;
        self.output.events.push(Event::Token { kind, n_raw_tokens });
    }

    /// Consumes `kind`, or reports that it is missing.
    pub(crate) fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.error(format!("expected {}", describe(kind)));
        false
    }

    pub(crate) fn error(&mut self, message: impl Into<String>) {
        let index = self.output.errors.len() as u32;
        self.output.errors.push(message.into());
        self.output.events.push(Event::Error { index });
    }

    /// Reports an error and, unless the next token is in `recovery` (or is
    /// a brace, which usually closes something the caller owns), skips it
    /// inside an `ERROR` node.
    pub(crate) fn err_recover(&mut self, message: &str, recovery: TokenSet) {
        if self.at_ts(recovery) || matches!(self.current(), T!['{'] | T!['}'] | EOF) {
            self.error(message);
            return;
        }
        self.err_and_bump(message);
    }

    pub(crate) fn err_and_bump(&mut self, message: &str) {
        let m = self.start();
        self.error(message);
        self.bump_any();
        m.complete(self, ERROR);
    }

    pub(crate) fn start(&mut self) -> Marker {
        let pos = self.output.events.len() as u32;
        self.output.events.push(Event::tombstone());
        Marker::new(pos)
    }
}

/// A node that has been started but not completed. Dropping one without
/// calling `complete` or `abandon` is a bug.
pub(crate) struct Marker {
    pos: u32,
    armed: bool,
}

impl Marker {
    fn new(pos: u32) -> Marker {
        Marker { pos, armed: true }
    }

    pub(crate) fn complete(mut self, p: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        self.armed = false;
        match &mut p.output.events[self.pos as usize] {
            Event::Start { kind: slot, .. } => *slot = kind,
            _ => unreachable!("marker does not point at a Start event"),
        }
        p.output.events.push(Event::Finish);
        CompletedMarker { pos: self.pos }
    }

    /// Forgets the node; its children become children of its parent.
    pub(crate) fn abandon(mut self, p: &mut Parser<'_>) {
        self.armed = false;
        let idx = self.pos as usize;
        if idx == p.output.events.len() - 1 {
            p.output.events.pop();
        }
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        if self.armed && !std::thread::panicking() {
            panic!("Marker must be completed or abandoned");
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CompletedMarker {
    pos: u32,
}

impl CompletedMarker {
    /// Starts a new node that will contain this completed one, as when
    /// `a` turns out to be the left side of `a + b`.
    pub(crate) fn precede(self, p: &mut Parser<'_>) -> Marker {
        let new = p.start();
        match &mut p.output.events[self.pos as usize] {
            Event::Start { forward_parent, .. } => {
                *forward_parent = NonZeroU32::new(new.pos - self.pos);
            }
            _ => unreachable!("completed marker does not point at a Start event"),
        }
        new
    }

    /// Makes this node start where `m` was started instead, so that it
    /// contains everything parsed after `m`, as when attributes turn out to
    /// belong to the expression that follows them.
    pub(crate) fn extend_to(self, p: &mut Parser<'_>, mut m: Marker) -> CompletedMarker {
        m.armed = false;
        match &mut p.output.events[m.pos as usize] {
            Event::Start { forward_parent, .. } => {
                *forward_parent = NonZeroU32::new(self.pos - m.pos);
            }
            _ => unreachable!("marker does not point at a Start event"),
        }
        self
    }
}

/// How a token is named in "expected ..." messages.
fn describe(kind: SyntaxKind) -> String {
    let text = match kind {
        IDENT => return "a name".to_string(),
        LIFETIME_IDENT => return "a lifetime".to_string(),
        INT_NUMBER => return "an integer".to_string(),
        SEMICOLON => ";",
        COMMA => ",",
        L_PAREN => "(",
        R_PAREN => ")",
        L_CURLY => "{",
        R_CURLY => "}",
        L_BRACK => "[",
        R_BRACK => "]",
        L_ANGLE => "<",
        R_ANGLE => ">",
        EQ => "=",
        COLON => ":",
        COLON2 => "::",
        FAT_ARROW => "=>",
        THIN_ARROW => "->",
        IN_KW => "in",
        _ => return format!("{kind:?}"),
    };
    format!("`{text}`")
}
