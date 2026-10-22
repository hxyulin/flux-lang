//! Tokens as macros see them: raw tokens (punctuation one character
//! each, as the lexer makes them) with where each came from, and views
//! of them glued into Rust's tokens (`::`, `=>`) and delimited groups.

use flux_intern::Symbol;
use flux_syntax::{SyntaxKind, SyntaxNode, T, TextRange, TokenOrigin};

/// One raw token of a macro's input or output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tok {
    pub kind: SyntaxKind,
    pub text: Symbol,
    /// Whether the next token touches it: `=` then `>` joint is `=>`.
    pub joint: bool,
    pub origin: TokenOrigin,
}

/// The group of tokens written in the file being lowered, outside any
/// macro definition: in order there.
pub(crate) const SOURCE_GROUP: u32 = 1;

impl Tok {
    pub(crate) fn range(&self) -> TextRange {
        self.origin.range
    }
}

/// The significant tokens of `node`, inner nodes included, each with its
/// origin: in a file, its range there in [`SOURCE_GROUP`]; in a macro
/// expansion's tree, where the expander said it came from.
pub(crate) fn tokens_of(node: SyntaxNode<'_>, out: &mut Vec<Tok>) {
    let tree = node.tree();
    let mut tokens = node.tokens().filter(|t| !t.kind().is_trivia()).peekable();
    let mut last_end = None;
    while let Some(token) = tokens.next() {
        let raw = token.raw_range();
        let origin = tree.token_origin(token.index()).unwrap_or(TokenOrigin {
            range: token.text_range(),
            group: SOURCE_GROUP,
            ctxt: 0,
        });
        let joint_next = tokens
            .peek()
            .is_some_and(|next| next.raw_range().start() == raw.end());
        let _ = last_end.replace(raw.end());
        let kind = token.kind();
        match kind.composite_parts() {
            // The parser glued it: split it again, joint.
            Some(parts) => {
                for (i, &part) in parts.iter().enumerate() {
                    let last = i + 1 == parts.len();
                    out.push(Tok {
                        kind: part,
                        text: Symbol::intern(punct_text(part)),
                        joint: !last || joint_next,
                        origin,
                    });
                }
            }
            None => out.push(Tok {
                kind,
                text: Symbol::intern(token.text()),
                joint: joint_next,
                origin,
            }),
        }
    }
}

/// The text of single-character punctuation.
pub(crate) fn punct_text(kind: SyntaxKind) -> &'static str {
    match kind {
        T![;] => ";",
        T![,] => ",",
        T!['('] => "(",
        T![')'] => ")",
        T!['{'] => "{",
        T!['}'] => "}",
        T!['['] => "[",
        T![']'] => "]",
        T![<] => "<",
        T![>] => ">",
        T![@] => "@",
        T![#] => "#",
        T![~] => "~",
        T![?] => "?",
        T![$] => "$",
        T![&] => "&",
        T![|] => "|",
        T![+] => "+",
        T![*] => "*",
        T![/] => "/",
        T![^] => "^",
        T![%] => "%",
        T![_] => "_",
        T![.] => ".",
        T![:] => ":",
        T![=] => "=",
        T![!] => "!",
        T![-] => "-",
        _ => "",
    }
}

/// A token as Rust's macros see it: raw tokens glued as rustc's lexer
/// glues them (`=` `>` joint is one `=>`), raw tokens `raw..raw + len`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Glued {
    pub kind: SyntaxKind,
    pub raw: u32,
    pub len: u32,
}

/// Composite punctuation, longest first.
const COMPOSITES: &[SyntaxKind] = &[
    T![...],
    T![..=],
    T![<<=],
    T![>>=],
    T![..],
    T![::],
    T![==],
    T![=>],
    T![!=],
    T![->],
    T![<=],
    T![>=],
    T![&&],
    T![||],
    T![<<],
    T![>>],
    T![+=],
    T![-=],
    T![*=],
    T![/=],
    T![%=],
    T![^=],
    T![&=],
    T![|=],
];

/// `toks` glued into Rust's tokens.
pub(crate) fn glue(toks: &[Tok]) -> Vec<Glued> {
    let mut out = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let mut glued = Glued {
            kind: toks[i].kind,
            raw: i as u32,
            len: 1,
        };
        if toks[i].kind.is_punct() {
            for &composite in COMPOSITES {
                let parts = composite.composite_parts().expect("composite");
                let fits = parts.iter().enumerate().all(|(j, &part)| {
                    toks.get(i + j).is_some_and(|t| t.kind == part)
                        && (j + 1 == parts.len() || toks[i + j].joint)
                });
                if fits {
                    glued = Glued {
                        kind: composite,
                        raw: i as u32,
                        len: parts.len() as u32,
                    };
                    break;
                }
            }
        }
        i += glued.len as usize;
        out.push(glued);
    }
    out
}

/// For each glued token that opens a group, the index of the one that
/// closes it; `u32::MAX` elsewhere and for unclosed groups.
pub(crate) fn match_delimiters(glued: &[Glued]) -> Vec<u32> {
    let mut closes = vec![u32::MAX; glued.len()];
    let mut open: Vec<usize> = Vec::new();
    for (i, g) in glued.iter().enumerate() {
        match g.kind {
            T!['('] | T!['['] | T!['{'] => open.push(i),
            T![')'] | T![']'] | T!['}'] => {
                if let Some(o) = open.pop() {
                    closes[o] = i as u32;
                }
            }
            _ => {}
        }
    }
    closes
}

pub(crate) fn is_open(kind: SyntaxKind) -> bool {
    matches!(kind, T!['('] | T!['['] | T!['{'])
}

pub(crate) fn is_close(kind: SyntaxKind) -> bool {
    matches!(kind, T![')'] | T![']'] | T!['}'])
}

/// Whether two glued tokens are the same token, as a matcher compares
/// them: of one kind and, for names and literals, one text.
pub(crate) fn same_token(a: &[Tok], ga: Glued, b: &[Tok], gb: Glued) -> bool {
    if ga.kind != gb.kind {
        return false;
    }
    match ga.kind {
        SyntaxKind::IDENT | SyntaxKind::LIFETIME_IDENT => {
            a[ga.raw as usize].text == b[gb.raw as usize].text
        }
        k if k.is_literal() => a[ga.raw as usize].text == b[gb.raw as usize].text,
        _ => true,
    }
}

/// How a token is named in messages: `` `x` ``, or the text of each part.
pub(crate) fn describe(toks: &[Tok], g: Glued) -> String {
    let mut s = String::new();
    for t in &toks[g.raw as usize..(g.raw + g.len) as usize] {
        s.push_str(t.text.as_str());
    }
    format!("`{s}`")
}

/// The text of `toks` with Rust's spacing for a macro expansion's tree:
/// a space between tokens, except punctuation that touched the next.
/// Each token's start offset in it goes to `starts`.
pub(crate) fn write_text(toks: &[Tok], starts: &mut Vec<u32>) -> String {
    let mut text = String::with_capacity(toks.len() * 4);
    let mut prev: Option<&Tok> = None;
    for t in toks {
        if let Some(p) = prev
            && !(p.joint && p.kind.is_punct() && t.kind.is_punct())
        {
            text.push(' ');
        }
        starts.push(text.len() as u32);
        text.push_str(t.text.as_str());
        prev = Some(t);
    }
    text
}

/// `toks` as source text with the spacing they were written with:
/// rustc's `stringify!`, approximately.
///
/// Tokens that came from the source are spaced as they were written
/// there, even after passing through other macros (`dbg!(a+b)`'s
/// `stringify!($val)`), and the parentheses that keep an expression
/// fragment one expression, which are nowhere in the source, are left
/// out, as rustc prints the fragment's expression.
pub(crate) fn pretty(toks: &[Tok]) -> String {
    let invisible = |t: &Tok| {
        matches!(t.kind, T!['('] | T![')'])
            && t.origin.range.is_empty()
            && t.origin.group != TokenOrigin::SYNTHETIC
    };
    let mut text = String::new();
    let mut prev: Option<&Tok> = None;
    for t in toks.iter().filter(|t| !invisible(t)) {
        if let Some(p) = prev {
            let from_source = p.origin.group == t.origin.group
                && p.origin.group != TokenOrigin::SYNTHETIC
                && p.origin.range.end() <= t.origin.range.start();
            let space = if from_source {
                p.origin.range.end() < t.origin.range.start()
            } else {
                !p.joint
            };
            if space {
                text.push(' ');
            }
        }
        text.push_str(t.text.as_str());
        prev = Some(t);
    }
    text
}
