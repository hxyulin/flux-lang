//! `macro_rules!` macros: reading a definition's rules, matching a
//! call's tokens against them as rustc's macro parser does, and
//! transcribing the rule that matched.
//!
//! Matching follows rustc's `mbe::macro_parser`: the matcher is
//! flattened into positions, and every way the tokens read so far can
//! match is followed at once, a token at a time. A fragment (`$e:expr`)
//! is parsed by the parser, which can only be asked for one at a time:
//! where several ways need one, or one needs a fragment and another a
//! token, the call is ambiguous, as in rustc.

use std::rc::Rc;

use flux_intern::Symbol;
use flux_syntax::parser::{self, Fragment};
use flux_syntax::{SyntaxKind, T, TextRange};
use rustc_hash::FxHashMap;

use super::tt::{Glued, Tok, describe, glue, is_close, is_open, match_delimiters, same_token};

/// What a metavariable matches: `$e:expr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FragKind {
    Expr,
    Ident,
    Ty,
    Pat,
    PatParam,
    Tt,
    Literal,
    Block,
    Stmt,
    Path,
    Lifetime,
    Vis,
    Item,
    Meta,
}

impl FragKind {
    fn from_name(name: &str) -> Option<FragKind> {
        Some(match name {
            "expr" | "expr_2021" => FragKind::Expr,
            "ident" => FragKind::Ident,
            "ty" => FragKind::Ty,
            "pat" => FragKind::Pat,
            "pat_param" => FragKind::PatParam,
            "tt" => FragKind::Tt,
            "literal" => FragKind::Literal,
            "block" => FragKind::Block,
            "stmt" => FragKind::Stmt,
            "path" => FragKind::Path,
            "lifetime" => FragKind::Lifetime,
            "vis" => FragKind::Vis,
            "item" => FragKind::Item,
            "meta" => FragKind::Meta,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            FragKind::Expr => "expr",
            FragKind::Ident => "ident",
            FragKind::Ty => "ty",
            FragKind::Pat => "pat",
            FragKind::PatParam => "pat_param",
            FragKind::Tt => "tt",
            FragKind::Literal => "literal",
            FragKind::Block => "block",
            FragKind::Stmt => "stmt",
            FragKind::Path => "path",
            FragKind::Lifetime => "lifetime",
            FragKind::Vis => "vis",
            FragKind::Item => "item",
            FragKind::Meta => "meta",
        }
    }

    /// The parser's fragment, for those the parser reads.
    fn fragment(self) -> Option<Fragment> {
        Some(match self {
            FragKind::Expr => Fragment::Expr,
            FragKind::Ty => Fragment::Type,
            FragKind::Pat => Fragment::Pat,
            FragKind::PatParam => Fragment::PatParam,
            FragKind::Block => Fragment::Block,
            FragKind::Stmt => Fragment::Stmt,
            FragKind::Path => Fragment::Path,
            FragKind::Vis => Fragment::Vis,
            FragKind::Item => Fragment::Item,
            FragKind::Meta => Fragment::Meta,
            FragKind::Ident | FragKind::Tt | FragKind::Literal | FragKind::Lifetime => {
                return None;
            }
        })
    }

    /// How rustc names the fragment in "expected" messages.
    fn expected(self) -> &'static str {
        match self {
            FragKind::Expr => "expression",
            FragKind::Ty => "type",
            FragKind::Pat | FragKind::PatParam => "pattern",
            FragKind::Block => "block",
            FragKind::Stmt => "statement",
            FragKind::Path => "path",
            FragKind::Item => "item",
            FragKind::Meta => "attribute",
            FragKind::Vis => "visibility",
            FragKind::Ident => "identifier",
            FragKind::Literal => "literal",
            FragKind::Lifetime => "lifetime",
            FragKind::Tt => "token tree",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// `*`
    ZeroOrMore,
    /// `+`
    OneOrMore,
    /// `?`
    ZeroOrOne,
}

/// A `macro_rules!` definition: its tokens and its rules, which refer to
/// them by index.
#[derive(Debug)]
pub struct Rules {
    pub(crate) toks: Box<[Tok]>,
    glued: Box<[Glued]>,
    rules: Box<[Rule]>,
    /// Some rule's matcher was invalid (reported where it is defined):
    /// calls are not expanded, nor reported again.
    pub invalid: bool,
}

#[derive(Debug)]
struct Rule {
    matcher: Box<[Loc]>,
    transcriber: Box<[Tt]>,
    /// The metavariables, in order of declaration.
    names: Box<[Symbol]>,
}

/// A position in a flattened matcher: rustc's `MatcherLoc`.
#[derive(Debug, Clone)]
enum Loc {
    /// A token to match, an index into the definition's glued tokens.
    Token(u32),
    /// The start of a delimited group; its tokens follow.
    Delimited,
    Sequence {
        op: Op,
        num_metavar_decls: usize,
        idx_first_after: usize,
        next_metavar: usize,
        seq_depth: usize,
    },
    SequenceKleeneOpNoSep {
        op: Op,
        idx_first: usize,
    },
    SequenceSep {
        sep: u32,
    },
    SequenceKleeneOpAfterSep {
        idx_first: usize,
    },
    MetaVarDecl {
        kind: FragKind,
        next_metavar: usize,
        seq_depth: usize,
    },
    Eof,
}

/// A rule's transcriber; tokens are indices into the definition's raw
/// tokens.
#[derive(Debug)]
enum Tt {
    Token(u32),
    Delimited {
        open: u32,
        close: u32,
        inner: Box<[Tt]>,
    },
    /// `$name`, written as `$` and the name.
    Var {
        name: Symbol,
        dollar: u32,
        ident: u32,
    },
    /// `$crate`.
    Crate {
        dollar: u32,
    },
    Sequence {
        inner: Box<[Tt]>,
        sep: Option<u32>,
        op: Op,
        range: TextRange,
    },
}

/// The definition's tokens as nested groups, in glued tokens.
enum Tree {
    Leaf(u32),
    Group {
        open: u32,
        close: Option<u32>,
        inner: Vec<Tree>,
    },
}

fn trees(glued: &[Glued], closes: &[u32], from: usize, to: usize) -> Vec<Tree> {
    let mut out = Vec::new();
    let mut i = from;
    while i < to {
        if is_open(glued[i].kind) {
            let close = closes[i];
            let end = if close == u32::MAX {
                to
            } else {
                close as usize
            };
            out.push(Tree::Group {
                open: i as u32,
                close: (close != u32::MAX).then_some(close),
                inner: trees(glued, closes, i + 1, end),
            });
            i = end + 1;
        } else {
            out.push(Tree::Leaf(i as u32));
            i += 1;
        }
    }
    out
}

/// A matcher as written, before it is flattened.
enum M {
    Token(u32),
    Group {
        open: u32,
        close: u32,
        inner: Vec<M>,
    },
    Var {
        name: Symbol,
        kind: FragKind,
        range: TextRange,
    },
    Sequence {
        inner: Vec<M>,
        sep: Option<u32>,
        op: Op,
    },
}

struct DefReader<'a> {
    toks: &'a [Tok],
    glued: &'a [Glued],
    errors: Vec<(TextRange, String)>,
}

impl DefReader<'_> {
    fn kind(&self, g: u32) -> SyntaxKind {
        self.glued[g as usize].kind
    }

    fn tok(&self, g: u32) -> &Tok {
        &self.toks[self.glued[g as usize].raw as usize]
    }

    fn range(&self, g: u32) -> TextRange {
        let g = self.glued[g as usize];
        let first = self.toks[g.raw as usize].origin;
        let last = self.toks[(g.raw + g.len - 1) as usize].origin;
        if first.group == last.group && first.range.start() <= last.range.end() {
            TextRange::new(first.range.start(), last.range.end())
        } else {
            first.range
        }
    }

    fn error(&mut self, range: TextRange, message: impl Into<String>) {
        self.errors.push((range, message.into()));
    }

    /// Whether glued token `g` is a name: `$name` and `$x:kind` take
    /// keywords too.
    fn is_name(&self, g: u32) -> bool {
        let k = self.kind(g);
        k == SyntaxKind::IDENT || k.is_keyword()
    }

    /// The repetition operator and separator after `$( .. )`, from
    /// `trees[*i]` on.
    fn kleene(
        &mut self,
        trees: &[Tree],
        i: &mut usize,
        at: TextRange,
    ) -> Option<(Option<u32>, Op)> {
        let op_of = |k: SyntaxKind| match k {
            T![*] => Some(Op::ZeroOrMore),
            T![+] => Some(Op::OneOrMore),
            T![?] => Some(Op::ZeroOrOne),
            _ => None,
        };
        let Some(Tree::Leaf(first)) = trees.get(*i) else {
            self.error(at, "expected one of: `*`, `+`, or `?`");
            return None;
        };
        let first = *first;
        if let Some(op) = op_of(self.kind(first)) {
            *i += 1;
            return Some((None, op));
        }
        if let Some(Tree::Leaf(second)) = trees.get(*i + 1)
            && let Some(op) = op_of(self.kind(*second))
        {
            let second = *second;
            *i += 2;
            if op == Op::ZeroOrOne {
                self.error(
                    self.range(second),
                    "the `?` macro repetition operator does not take a separator",
                );
                return None;
            }
            return Some((Some(first), op));
        }
        self.error(self.range(first), "expected one of: `*`, `+`, or `?`");
        None
    }

    fn matcher(&mut self, trees: &[Tree], names: &mut Vec<Symbol>) -> Vec<M> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < trees.len() {
            match &trees[i] {
                Tree::Leaf(g) if self.kind(*g) == T![$] => {
                    let dollar = *g;
                    match trees.get(i + 1) {
                        Some(Tree::Leaf(name)) if self.is_name(*name) => {
                            let name_g = *name;
                            let name = self.tok(name_g).text;
                            let start = self.range(dollar).start();
                            let has_colon = matches!(trees.get(i + 2), Some(Tree::Leaf(c)) if self.kind(*c) == T![:]);
                            let spec = match trees.get(i + 3) {
                                Some(Tree::Leaf(s)) if has_colon && self.is_name(*s) => Some(*s),
                                _ => None,
                            };
                            let Some(spec) = spec else {
                                let range =
                                    TextRange::new(start, self.range(name_g).end().max(start));
                                self.error(range, "missing fragment specifier");
                                i += if has_colon { 3 } else { 2 };
                                continue;
                            };
                            let end = self.range(spec).end().max(start);
                            let range = TextRange::new(start, end);
                            let spec_text = self.tok(spec).text;
                            let Some(kind) = FragKind::from_name(spec_text.as_str()) else {
                                self.error(
                                    self.range(spec),
                                    format!("invalid fragment specifier `{spec_text}`"),
                                );
                                i += 4;
                                continue;
                            };
                            if names.contains(&name) {
                                self.error(range, "duplicate matcher binding");
                            }
                            names.push(name);
                            out.push(M::Var { name, kind, range });
                            i += 4;
                        }
                        Some(Tree::Group { open, inner, .. }) if self.kind(*open) == T!['('] => {
                            let at = self.range(*open);
                            let inner = self.matcher(inner, names);
                            i += 2;
                            if let Some((sep, op)) = self.kleene(trees, &mut i, at) {
                                out.push(M::Sequence { inner, sep, op });
                            }
                        }
                        // At the end of a matcher group `$` is a literal,
                        // as in the compiler's `T![$]` rule.
                        None => {
                            out.push(M::Token(dollar));
                            i += 1;
                        }
                        _ => {
                            self.error(self.range(dollar), "expected identifier, found `$`");
                            i += 1;
                        }
                    }
                }
                Tree::Leaf(g) => {
                    out.push(M::Token(*g));
                    i += 1;
                }
                Tree::Group { open, close, inner } => {
                    let inner = self.matcher(inner, names);
                    out.push(M::Group {
                        open: *open,
                        close: close.unwrap_or(*open),
                        inner,
                    });
                    i += 1;
                }
            }
        }
        out
    }

    fn transcriber(&mut self, trees: &[Tree]) -> Vec<Tt> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < trees.len() {
            match &trees[i] {
                Tree::Leaf(g) if self.kind(*g) == T![$] => {
                    let dollar = self.glued[*g as usize].raw;
                    match trees.get(i + 1) {
                        Some(Tree::Leaf(name)) if self.kind(*name) == T![crate] => {
                            out.push(Tt::Crate { dollar });
                            i += 2;
                        }
                        Some(Tree::Leaf(name)) if self.is_name(*name) => {
                            out.push(Tt::Var {
                                name: self.tok(*name).text,
                                dollar,
                                ident: self.glued[*name as usize].raw,
                            });
                            i += 2;
                        }
                        Some(Tree::Group { open, close, inner }) if self.kind(*open) == T!['('] => {
                            let at = self.range(*open);
                            let start = at.start();
                            let inner = self.transcriber(inner);
                            i += 2;
                            let last = close.unwrap_or(*open);
                            if let Some((sep, op)) = self.kleene(trees, &mut i, at) {
                                let end = match trees.get(i - 1) {
                                    Some(Tree::Leaf(op)) => self.range(*op).end(),
                                    _ => self.range(last).end(),
                                };
                                out.push(Tt::Sequence {
                                    inner: inner.into(),
                                    sep,
                                    op,
                                    range: TextRange::new(start, end.max(start)),
                                });
                            }
                        }
                        _ => {
                            out.push(Tt::Token(dollar));
                            i += 1;
                        }
                    }
                }
                Tree::Leaf(g) => {
                    let g = self.glued[*g as usize];
                    for raw in g.raw..g.raw + g.len {
                        out.push(Tt::Token(raw));
                    }
                    i += 1;
                }
                Tree::Group { open, close, inner } => {
                    let inner = self.transcriber(inner).into();
                    let open_raw = self.glued[*open as usize].raw;
                    out.push(Tt::Delimited {
                        open: open_raw,
                        close: close.map_or(open_raw, |c| self.glued[c as usize].raw),
                        inner,
                    });
                    i += 1;
                }
            }
        }
        out
    }

    // The follow-set rules: what may come after a fragment, as rustc's
    // `check_matcher` checks them.

    /// The tokens and fragments that can start `ms`, and whether it can
    /// match nothing.
    fn first(&self, ms: &[M], out: &mut Vec<Follow>) -> bool {
        for m in ms {
            match m {
                M::Token(g) => {
                    out.push(Follow::Token(*g));
                    return false;
                }
                M::Group { open, .. } => {
                    out.push(Follow::Token(*open));
                    return false;
                }
                M::Var { kind, range, name } => {
                    out.push(Follow::Var(*kind, *name, *range));
                    if *kind != FragKind::Vis {
                        return false;
                    }
                }
                M::Sequence { inner, op, .. } => {
                    let empty = self.first(inner, out);
                    if !empty && *op == Op::OneOrMore {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Checks that each fragment in `ms` is followed only by what its
    /// kind allows, `follow` being what may come after `ms`.
    fn check_follow(&mut self, ms: &[M], follow: &[Follow]) {
        for (i, m) in ms.iter().enumerate() {
            let rest = |this: &Self| -> Vec<Follow> {
                let mut f = Vec::new();
                if this.first(&ms[i + 1..], &mut f) {
                    f.extend_from_slice(follow);
                }
                f
            };
            match m {
                M::Token(_) => {}
                M::Group { close, inner, .. } => {
                    self.check_follow(inner, &[Follow::Token(*close)]);
                }
                M::Sequence { inner, sep, op } => {
                    let mut after = rest(self);
                    match sep {
                        Some(sep) => after.push(Follow::Token(*sep)),
                        None if *op != Op::ZeroOrOne => {
                            self.first(inner, &mut after);
                        }
                        None => {}
                    }
                    self.check_follow(inner, &after);
                }
                M::Var { name, kind, .. } => {
                    for f in rest(self) {
                        if let Some(message) = self.not_allowed(*name, *kind, f) {
                            self.error(message.0, message.1);
                            // rustc reports each fragment once.
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Whether `f` may follow a fragment of kind `kind`.
    fn not_allowed(&self, name: Symbol, kind: FragKind, f: Follow) -> Option<(TextRange, String)> {
        let what = |s: &str| {
            format!(
                "`${name}:{}` is followed by {s}, which is not allowed for `{}` fragments",
                kind.name(),
                kind.name()
            )
        };
        let token_ok = |k: SyntaxKind, text: &str| match kind {
            FragKind::Expr | FragKind::Stmt => is_close(k) || matches!(k, T![=>] | T![,] | T![;]),
            FragKind::Pat => is_close(k) || matches!(k, T![=>] | T![,] | T![=] | T![if] | T![in]),
            FragKind::PatParam => {
                is_close(k) || matches!(k, T![=>] | T![,] | T![=] | T![|] | T![if] | T![in])
            }
            FragKind::Path | FragKind::Ty => {
                is_close(k)
                    || matches!(
                        k,
                        T!['{']
                            | T!['[']
                            | T![=>]
                            | T![,]
                            | T![>]
                            | T![=]
                            | T![:]
                            | T![;]
                            | T![|]
                            | T![as]
                            | T![where]
                    )
            }
            FragKind::Vis => {
                k == T![,]
                    || is_close(k)
                    || (k == SyntaxKind::IDENT && text != "priv")
                    || (k.is_keyword() && k != SyntaxKind::PRIV_KW)
                    || can_begin_type(k)
            }
            _ => true,
        };
        match f {
            Follow::Token(g) => {
                let k = self.kind(g);
                if token_ok(k, self.tok(g).text.as_str()) {
                    return None;
                }
                Some((
                    self.range(g),
                    what(&describe(self.toks, self.glued[g as usize])),
                ))
            }
            Follow::Var(other, other_name, range) => {
                let ok = match kind {
                    FragKind::Expr | FragKind::Stmt | FragKind::Pat | FragKind::PatParam => false,
                    FragKind::Path | FragKind::Ty => other == FragKind::Block,
                    FragKind::Vis => {
                        matches!(other, FragKind::Ident | FragKind::Ty | FragKind::Path)
                    }
                    _ => true,
                };
                if ok {
                    return None;
                }
                Some((range, what(&format!("`${other_name}:{}`", other.name()))))
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Follow {
    Token(u32),
    Var(FragKind, Symbol, TextRange),
}

fn flatten(ms: &[M], locs: &mut Vec<Loc>, next_metavar: &mut usize, seq_depth: usize) {
    for m in ms {
        match m {
            M::Token(g) => locs.push(Loc::Token(*g)),
            M::Group { open, close, inner } => {
                locs.push(Loc::Delimited);
                locs.push(Loc::Token(*open));
                flatten(inner, locs, next_metavar, seq_depth);
                locs.push(Loc::Token(*close));
            }
            M::Sequence { inner, sep, op } => {
                let idx_seq = locs.len();
                locs.push(Loc::Eof);
                let next_metavar_orig = *next_metavar;
                let idx_first = locs.len();
                flatten(inner, locs, next_metavar, seq_depth + 1);
                match sep {
                    Some(sep) => {
                        locs.push(Loc::SequenceSep { sep: *sep });
                        locs.push(Loc::SequenceKleeneOpAfterSep { idx_first });
                    }
                    None => locs.push(Loc::SequenceKleeneOpNoSep { op: *op, idx_first }),
                }
                locs[idx_seq] = Loc::Sequence {
                    op: *op,
                    num_metavar_decls: *next_metavar - next_metavar_orig,
                    idx_first_after: locs.len(),
                    next_metavar: next_metavar_orig,
                    seq_depth,
                };
            }
            M::Var { kind, .. } => {
                locs.push(Loc::MetaVarDecl {
                    kind: *kind,
                    next_metavar: *next_metavar,
                    seq_depth,
                });
                *next_metavar += 1;
            }
        }
    }
}

/// Reads the rules of `macro_rules! name { body }` from the body's
/// tokens, its braces left out; errors are at `range` when they have no
/// token of their own.
pub(crate) fn parse_rules(toks: Vec<Tok>, range: TextRange) -> (Rules, Vec<(TextRange, String)>) {
    let glued = glue(&toks);
    let closes = match_delimiters(&glued);
    let all = trees(&glued, &closes, 0, glued.len());
    let mut reader = DefReader {
        toks: &toks,
        glued: &glued,
        errors: Vec::new(),
    };
    let mut rules = Vec::new();

    let mut i = 0;
    while i < all.len() {
        let Tree::Group {
            inner: matcher,
            open,
            ..
        } = &all[i]
        else {
            let Tree::Leaf(g) = all[i] else {
                unreachable!()
            };
            reader.error(reader.range(g), "expected macro matcher in delimiters");
            break;
        };
        let open = *open;
        let arrow = matches!(all.get(i + 1), Some(Tree::Leaf(g)) if reader.kind(*g) == T![=>]);
        if !arrow {
            let at = match all.get(i + 1) {
                Some(Tree::Leaf(g)) => reader.range(*g),
                _ => reader.range(open),
            };
            reader.error(at, "expected `=>`");
            break;
        }
        let Some(Tree::Group {
            inner: transcriber, ..
        }) = all.get(i + 2)
        else {
            reader.error(
                reader.range(open),
                "expected macro transcriber in delimiters",
            );
            break;
        };
        let mut names = Vec::new();
        let matcher = reader.matcher(matcher, &mut names);
        reader.check_follow(&matcher, &[]);
        let mut locs = Vec::new();
        let mut next_metavar = 0;
        flatten(&matcher, &mut locs, &mut next_metavar, 0);
        locs.push(Loc::Eof);
        let transcriber = reader.transcriber(transcriber);
        rules.push(Rule {
            matcher: locs.into(),
            transcriber: transcriber.into(),
            names: names.into(),
        });

        i += 3;
        match all.get(i) {
            None => {}
            Some(Tree::Leaf(g)) if reader.kind(*g) == T![;] => i += 1,
            Some(Tree::Leaf(g)) => {
                reader.error(reader.range(*g), "expected `;`");
                break;
            }
            Some(Tree::Group { open, .. }) => {
                reader.error(reader.range(*open), "expected `;`");
                break;
            }
        }
    }
    if rules.is_empty() && reader.errors.is_empty() {
        reader.error(range, "macros must contain at least one rule");
    }
    let errors = reader.errors;
    let rules = Rules {
        invalid: !errors.is_empty(),
        toks: toks.into(),
        glued: glued.into(),
        rules: rules.into(),
    };
    (rules, errors)
}

/// What a metavariable matched.
#[derive(Debug, Clone)]
pub(crate) enum NamedMatch {
    Seq(Vec<NamedMatch>),
    /// Glued input tokens `start..end`.
    Single {
        start: u32,
        end: u32,
        kind: FragKind,
    },
}

/// A call's tokens, as a matcher reads them.
pub(crate) struct CallInput<'a> {
    pub toks: &'a [Tok],
    pub glued: Vec<Glued>,
    closes: Vec<u32>,
    parser_input: parser::Input,
    /// For each raw token, the glued token it starts, or `u32::MAX`.
    raw_to_glued: Vec<u32>,
    /// Where the input ends: the call's closing delimiter.
    eof: TextRange,
}

impl<'a> CallInput<'a> {
    pub(crate) fn new(toks: &'a [Tok], eof: TextRange) -> CallInput<'a> {
        let glued = glue(toks);
        let closes = match_delimiters(&glued);
        let mut parser_input = parser::Input::default();
        for t in toks {
            parser_input.push(t.kind, t.is_union_keyword());
            if t.joint {
                parser_input.set_last_joint();
            }
        }
        let mut raw_to_glued = vec![u32::MAX; toks.len() + 1];
        for (i, g) in glued.iter().enumerate() {
            raw_to_glued[g.raw as usize] = i as u32;
        }
        raw_to_glued[toks.len()] = glued.len() as u32;
        CallInput {
            toks,
            glued,
            closes,
            parser_input,
            raw_to_glued,
            eof,
        }
    }

    /// The range of glued token `g`.
    pub(crate) fn range(&self, g: usize) -> TextRange {
        let g = self.glued[g];
        let first = self.toks[g.raw as usize].origin;
        let last = self.toks[(g.raw + g.len - 1) as usize].origin;
        if first.group == last.group && first.range.start() <= last.range.end() {
            TextRange::new(first.range.start(), last.range.end())
        } else {
            first.range
        }
    }

    /// Raw tokens of glued tokens `start..end`.
    pub(crate) fn raw(&self, start: u32, end: u32) -> &'a [Tok] {
        if start >= end {
            return &[];
        }
        let a = self.glued[start as usize].raw as usize;
        let last = self.glued[end as usize - 1];
        &self.toks[a..(last.raw + last.len) as usize]
    }

    /// How many glued tokens from `pos` a fragment of kind `kind` takes;
    /// an error with what was expected if they do not make one.
    fn fragment(&self, pos: usize, kind: FragKind) -> Result<usize, (TextRange, String)> {
        let at = |k: SyntaxKind| self.glued.get(pos).is_some_and(|g| g.kind == k);
        match kind {
            FragKind::Tt => {
                if is_open(self.glued[pos].kind) {
                    let close = self.closes[pos];
                    if close == u32::MAX {
                        return Err((self.range(pos), "unclosed delimiter".into()));
                    }
                    return Ok(close as usize + 1 - pos);
                }
                Ok(1)
            }
            FragKind::Ident | FragKind::Lifetime => Ok(1),
            FragKind::Literal => Ok(if at(T![-]) { 2 } else { 1 }),
            _ => {
                let fragment = kind.fragment().expect("parsed fragments");
                let raw = self.glued[pos].raw as usize;
                let n = match parser::fragment_or_error(&self.parser_input, raw, fragment) {
                    Ok(n) => n,
                    Err(at) => {
                        let found = (at..self.toks.len())
                            .find(|&r| self.raw_to_glued[r] != u32::MAX)
                            .filter(|&r| r < self.toks.len());
                        return Err(match found {
                            Some(r) => {
                                let g = self.raw_to_glued[r] as usize;
                                let t = &self.toks[r];
                                (
                                    self.range(g),
                                    format!(
                                        "expected {}, found {}",
                                        kind.expected(),
                                        super::describe_kind(t.kind, t.text.as_str())
                                    ),
                                )
                            }
                            None => (
                                self.eof,
                                format!(
                                    "expected {}, found end of macro arguments",
                                    kind.expected()
                                ),
                            ),
                        });
                    }
                };
                let end = self.raw_to_glued[raw + n];
                if end == u32::MAX {
                    return Err((self.range(pos), format!("expected {}", kind.expected())));
                }
                Ok(end as usize - pos)
            }
        }
    }
}

fn can_begin_expr(k: SyntaxKind) -> bool {
    k == SyntaxKind::IDENT
        || k == SyntaxKind::LIFETIME_IDENT
        || k.is_literal()
        || matches!(
            k,
            T!['(']
                | T!['[']
                | T!['{']
                | T![!]
                | T![-]
                | T![*]
                | T![|]
                | T![||]
                | T![&]
                | T![&&]
                | T![..]
                | T![...]
                | T![..=]
                | T![<]
                | T![<<]
                | T![::]
                | T![#]
                | T![self]
                | T![Self]
                | T![super]
                | T![crate]
                | T![async]
                | T![break]
                | T![const]
                | T![continue]
                | T![false]
                | T![for]
                | T![if]
                | T![loop]
                | T![match]
                | T![move]
                | T![return]
                | T![true]
                | T![unsafe]
                | T![while]
                | T![static]
                | SyntaxKind::DO_KW
                | SyntaxKind::BOX_KW
                | SyntaxKind::YIELD_KW
                | SyntaxKind::TRY_KW
                | SyntaxKind::GEN_KW
        )
}

fn can_begin_type(k: SyntaxKind) -> bool {
    k == SyntaxKind::IDENT
        || k == SyntaxKind::LIFETIME_IDENT
        || matches!(
            k,
            T!['(']
                | T!['[']
                | T![!]
                | T![*]
                | T![&]
                | T![&&]
                | T![?]
                | T![<]
                | T![<<]
                | T![::]
                | T![self]
                | T![Self]
                | T![super]
                | T![crate]
                | T![_]
                | T![for]
                | T![impl]
                | T![fn]
                | T![unsafe]
                | T![extern]
                | T![dyn]
                | SyntaxKind::TYPEOF_KW
        )
}

/// Whether a token of kind `k` can start a fragment of kind `kind`:
/// rustc's `nonterminal_may_begin_with`. A matcher does not try to read
/// a fragment at a token that cannot start one.
fn may_begin(kind: FragKind, k: SyntaxKind) -> bool {
    let is_name = k == SyntaxKind::IDENT || k.is_keyword();
    match kind {
        FragKind::Expr => can_begin_expr(k) && k != T![let],
        FragKind::Ty => can_begin_type(k),
        FragKind::Ident => is_name,
        FragKind::Literal => k.is_literal() || matches!(k, T![-] | T![true] | T![false]),
        FragKind::Vis => k == T![,] || is_name || can_begin_type(k),
        FragKind::Block => k == T!['{'],
        FragKind::Path | FragKind::Meta => is_name || k == T![::],
        FragKind::Pat | FragKind::PatParam => {
            is_name
                || k.is_literal()
                || matches!(
                    k,
                    T!['(']
                        | T!['[']
                        | T![&]
                        | T![&&]
                        | T![..]
                        | T![...]
                        | T![::]
                        | T![<]
                        | T![<<]
                        | T![-]
                        | T![_]
                )
                || (k == T![|] && kind == FragKind::Pat)
        }
        FragKind::Lifetime => k == SyntaxKind::LIFETIME_IDENT,
        FragKind::Tt | FragKind::Item | FragKind::Stmt => !is_close(k),
    }
}

/// A way the tokens read so far can match: rustc's `MatcherPos`.
#[derive(Clone)]
struct MatcherPos {
    idx: usize,
    matches: Rc<Vec<NamedMatch>>,
}

impl MatcherPos {
    fn push_match(&mut self, metavar_idx: usize, seq_depth: usize, m: NamedMatch) {
        let matches = Rc::make_mut(&mut self.matches);
        if seq_depth == 0 {
            debug_assert_eq!(metavar_idx, matches.len());
            matches.push(m);
            return;
        }
        let mut curr = &mut matches[metavar_idx];
        for _ in 0..seq_depth - 1 {
            match curr {
                NamedMatch::Seq(seq) => curr = seq.last_mut().expect("an open sequence"),
                NamedMatch::Single { .. } => unreachable!("a sequence at this depth"),
            }
        }
        match curr {
            NamedMatch::Seq(seq) => seq.push(m),
            NamedMatch::Single { .. } => unreachable!("a sequence at this depth"),
        }
    }
}

/// Why a rule did not match.
pub(crate) enum MatchError {
    /// It does not match from glued token `at` on (the end, if it is
    /// the input's length): another rule may.
    Failure { at: usize },
    /// The call is wrong whatever the other rules say.
    Error { range: TextRange, message: String },
}

impl Rules {
    pub(crate) fn len(&self) -> usize {
        self.rules.len()
    }

    /// Matches `input` against rule `rule`: what each metavariable
    /// matched, by name.
    pub(crate) fn match_rule(
        &self,
        rule: usize,
        input: &CallInput<'_>,
        macro_name: Symbol,
    ) -> Result<FxHashMap<Symbol, NamedMatch>, MatchError> {
        let names = &self.rules[rule].names;
        let matches = self.match_positions(rule, input, macro_name)?;
        Ok(names.iter().copied().zip(matches).collect())
    }

    fn match_positions(
        &self,
        rule: usize,
        input: &CallInput<'_>,
        macro_name: Symbol,
    ) -> Result<Vec<NamedMatch>, MatchError> {
        let names = &self.rules[rule].names;
        let matcher = &self.rules[rule].matcher;
        let mut cur = vec![MatcherPos {
            idx: 0,
            matches: Rc::new(Vec::new()),
        }];
        let mut next: Vec<MatcherPos> = Vec::new();
        let mut bb: Vec<MatcherPos> = Vec::new();
        let mut pos = 0usize;
        loop {
            next.clear();
            bb.clear();
            let token = input.glued.get(pos).copied();
            let mut eof: Option<MatcherPos> = None;
            let mut eof_count = 0;
            while let Some(mut mp) = cur.pop() {
                match &matcher[mp.idx] {
                    &Loc::Token(t) => {
                        if let Some(token) = token
                            && same_token(&self.toks, self.glued[t as usize], input.toks, token)
                        {
                            mp.idx += 1;
                            next.push(mp);
                        }
                    }
                    Loc::Delimited => {
                        mp.idx += 1;
                        cur.push(mp);
                    }
                    &Loc::Sequence {
                        op,
                        num_metavar_decls,
                        idx_first_after,
                        next_metavar,
                        seq_depth,
                    } => {
                        for metavar_idx in next_metavar..next_metavar + num_metavar_decls {
                            mp.push_match(metavar_idx, seq_depth, NamedMatch::Seq(Vec::new()));
                        }
                        if matches!(op, Op::ZeroOrMore | Op::ZeroOrOne) {
                            cur.push(MatcherPos {
                                idx: idx_first_after,
                                matches: Rc::clone(&mp.matches),
                            });
                        }
                        mp.idx += 1;
                        cur.push(mp);
                    }
                    &Loc::SequenceKleeneOpNoSep { op, idx_first } => {
                        cur.push(MatcherPos {
                            idx: mp.idx + 1,
                            matches: Rc::clone(&mp.matches),
                        });
                        if op != Op::ZeroOrOne {
                            mp.idx = idx_first;
                            cur.push(mp);
                        }
                    }
                    &Loc::SequenceSep { sep } => {
                        cur.push(MatcherPos {
                            idx: mp.idx + 2,
                            matches: Rc::clone(&mp.matches),
                        });
                        if let Some(token) = token
                            && same_token(&self.toks, self.glued[sep as usize], input.toks, token)
                        {
                            mp.idx += 1;
                            next.push(mp);
                        }
                    }
                    &Loc::SequenceKleeneOpAfterSep { idx_first } => {
                        mp.idx = idx_first;
                        cur.push(mp);
                    }
                    &Loc::MetaVarDecl { kind, .. } => {
                        if let Some(token) = token
                            && may_begin(kind, token.kind)
                            && !(kind == FragKind::Ident
                                && token.kind == T![crate]
                                && super::definition_crate(
                                    input.toks[token.raw as usize].origin.ctxt,
                                )
                                .is_some())
                        {
                            bb.push(mp);
                        }
                    }
                    Loc::Eof => {
                        if token.is_none() {
                            eof_count += 1;
                            eof = Some(mp);
                        }
                    }
                }
            }
            if token.is_none() {
                return match (eof_count, eof) {
                    (1, Some(mp)) => Ok(Rc::unwrap_or_clone(mp.matches)),
                    (0, _) => Err(MatchError::Failure { at: pos }),
                    _ => Err(MatchError::Error {
                        range: input.range(input.glued.len().saturating_sub(1)),
                        message: "ambiguity: multiple successful parses".into(),
                    }),
                };
            }
            match (next.len(), bb.len()) {
                (0, 0) => return Err(MatchError::Failure { at: pos }),
                (_, 0) => {
                    std::mem::swap(&mut cur, &mut next);
                    pos += 1;
                }
                (0, 1) => {
                    let mut mp = bb.pop().expect("one");
                    let Loc::MetaVarDecl {
                        kind,
                        next_metavar,
                        seq_depth,
                    } = matcher[mp.idx]
                    else {
                        unreachable!("only metavariables wait for the parser")
                    };
                    let n = input
                        .fragment(pos, kind)
                        .map_err(|(range, message)| MatchError::Error { range, message })?;
                    mp.push_match(
                        next_metavar,
                        seq_depth,
                        NamedMatch::Single {
                            start: pos as u32,
                            end: (pos + n) as u32,
                            kind,
                        },
                    );
                    mp.idx += 1;
                    pos += n;
                    cur.push(mp);
                }
                _ => {
                    let options: Vec<String> = bb
                        .iter()
                        .map(|mp| match matcher[mp.idx] {
                            Loc::MetaVarDecl {
                                kind, next_metavar, ..
                            } => format!(
                                "{} ('{}')",
                                kind.name(),
                                names.get(next_metavar).copied().unwrap_or(macro_name)
                            ),
                            _ => unreachable!(),
                        })
                        .collect();
                    let mut message = format!(
                        "local ambiguity when calling macro `{macro_name}`: multiple parsing options: "
                    );
                    if next.is_empty() {
                        message.push_str(&format!("built-in NTs {}.", options.join(" or ")));
                    } else {
                        message.push_str(&format!(
                            "built-in NTs {} or {} other option{}.",
                            options.join(" or "),
                            next.len(),
                            if next.len() == 1 { "" } else { "s" }
                        ));
                    }
                    return Err(MatchError::Error {
                        range: input.range(pos),
                        message,
                    });
                }
            }
        }
    }

    /// The tokens rule `rule`'s transcriber makes with `bindings`;
    /// `def` turns the definition's token `i` into the output's.
    pub(crate) fn transcribe(
        &self,
        rule: usize,
        input: &CallInput<'_>,
        bindings: &FxHashMap<Symbol, NamedMatch>,
        def: &mut dyn FnMut(&Tok, &mut Vec<Tok>),
        dollar_crate: &mut dyn FnMut(&Tok, &mut Vec<Tok>),
    ) -> Result<Vec<Tok>, (TextRange, String)> {
        let mut out = Vec::new();
        let mut repeats = Vec::new();
        let mut t = Transcriber {
            rules: self,
            input,
            bindings,
            def,
            dollar_crate,
        };
        t.transcribe(&self.rules[rule].transcriber, &mut repeats, &mut out)?;
        Ok(out)
    }
}

struct Transcriber<'a, 'b> {
    rules: &'a Rules,
    input: &'a CallInput<'b>,
    bindings: &'a FxHashMap<Symbol, NamedMatch>,
    def: &'a mut dyn FnMut(&Tok, &mut Vec<Tok>),
    dollar_crate: &'a mut dyn FnMut(&Tok, &mut Vec<Tok>),
}

/// How many times a repetition repeats, from the metavariables in it.
enum Size {
    Unconstrained,
    Constraint(usize, Symbol),
    Contradiction(String),
}

impl Size {
    fn with(self, other: Size) -> Size {
        match (self, other) {
            (Size::Unconstrained, o) | (o, Size::Unconstrained) => o,
            (c @ Size::Contradiction(_), _) | (_, c @ Size::Contradiction(_)) => c,
            (Size::Constraint(a, an), Size::Constraint(b, bn)) => {
                if a == b {
                    Size::Constraint(a, an)
                } else {
                    let times = |n: usize| if n == 1 { "time" } else { "times" };
                    Size::Contradiction(format!(
                        "meta-variable `{an}` repeats {a} {}, but `{bn}` repeats {b} {}",
                        times(a),
                        times(b)
                    ))
                }
            }
        }
    }
}

impl Transcriber<'_, '_> {
    fn lookup(&self, name: Symbol, repeats: &[(usize, usize)]) -> Option<&NamedMatch> {
        let mut m = self.bindings.get(&name)?;
        for &(idx, _) in repeats {
            match m {
                NamedMatch::Seq(seq) => m = seq.get(idx)?,
                NamedMatch::Single { .. } => break,
            }
        }
        Some(m)
    }

    fn size(&self, tts: &[Tt], repeats: &[(usize, usize)]) -> Size {
        let mut size = Size::Unconstrained;
        for tt in tts {
            let s = match tt {
                Tt::Token(_) | Tt::Crate { .. } => Size::Unconstrained,
                Tt::Delimited { inner, .. } | Tt::Sequence { inner, .. } => {
                    self.size(inner, repeats)
                }
                Tt::Var { name, .. } => match self.lookup(*name, repeats) {
                    Some(NamedMatch::Seq(seq)) => Size::Constraint(seq.len(), *name),
                    _ => Size::Unconstrained,
                },
            };
            size = size.with(s);
        }
        size
    }

    fn transcribe(
        &mut self,
        tts: &[Tt],
        repeats: &mut Vec<(usize, usize)>,
        out: &mut Vec<Tok>,
    ) -> Result<(), (TextRange, String)> {
        let toks = &self.rules.toks;
        for tt in tts {
            match tt {
                &Tt::Token(i) => (self.def)(&toks[i as usize], out),
                &Tt::Delimited {
                    open,
                    close,
                    ref inner,
                } => {
                    (self.def)(&toks[open as usize], out);
                    self.transcribe(inner, repeats, out)?;
                    (self.def)(&toks[close as usize], out);
                }
                &Tt::Crate { dollar } => (self.dollar_crate)(&toks[dollar as usize], out),
                &Tt::Var {
                    name,
                    dollar,
                    ident,
                } => match self.lookup(name, repeats) {
                    None => {
                        (self.def)(&toks[dollar as usize], out);
                        (self.def)(&toks[ident as usize], out);
                    }
                    Some(NamedMatch::Seq(_)) => {
                        let at = toks[dollar as usize].origin.range;
                        return Err((
                            at,
                            format!("variable `{name}` is still repeating at this depth"),
                        ));
                    }
                    Some(&NamedMatch::Single { start, end, kind }) => {
                        let raw = self.input.raw(start, end);
                        // An expression stays one when put among other
                        // tokens: `$e * 2` multiplies all of `$e`.
                        let wrap = kind == FragKind::Expr && end - start > 1;
                        if wrap {
                            let first = raw[0].origin;
                            out.push(Tok {
                                kind: T!['('],
                                text: Symbol::intern("("),
                                joint: false,
                                origin: flux_syntax::TokenOrigin {
                                    range: TextRange::empty(first.range.start()),
                                    ..first
                                },
                            });
                        }
                        out.extend_from_slice(raw);
                        if let Some(last) = out.last_mut() {
                            last.joint = false;
                        }
                        if wrap {
                            let last = raw[raw.len() - 1].origin;
                            out.push(Tok {
                                kind: T![')'],
                                text: Symbol::intern(")"),
                                joint: false,
                                origin: flux_syntax::TokenOrigin {
                                    range: TextRange::empty(last.range.end()),
                                    ..last
                                },
                            });
                        }
                    }
                },
                Tt::Sequence {
                    inner,
                    sep,
                    op,
                    range,
                } => {
                    let len = match self.size(inner, repeats) {
                        Size::Unconstrained => {
                            return Err((
                                *range,
                                "attempted to repeat an expression containing no syntax variables matched as repeating at this depth".into(),
                            ));
                        }
                        Size::Contradiction(message) => return Err((*range, message)),
                        Size::Constraint(len, _) => len,
                    };
                    if len == 0 && *op == Op::OneOrMore {
                        return Err((*range, "this must repeat at least once".into()));
                    }
                    for i in 0..len {
                        if i > 0
                            && let Some(sep) = sep
                        {
                            let g = self.rules.glued[*sep as usize];
                            for raw in g.raw..g.raw + g.len {
                                (self.def)(&toks[raw as usize], out);
                            }
                        }
                        repeats.push((i, len));
                        let result = self.transcribe(inner, repeats, out);
                        repeats.pop();
                        result?;
                    }
                }
            }
        }
        Ok(())
    }
}
