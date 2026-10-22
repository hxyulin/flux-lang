//! Macros the compiler implements, which `core` declares with
//! `#[rustc_builtin_macro]`: `assert!`, `panic!`, `format_args!` and
//! friends, as rustc's expand them for edition 2024.
//!
//! flux has no formatting machinery yet: a panic aborts without its
//! message, so `format_args!` checks its format string and that each
//! argument implements the trait its placeholder names, evaluates the
//! arguments once each, and makes an `Arguments` that holds only the
//! string's literal pieces.

use flux_intern::Symbol;
use flux_syntax::parser::{self, Fragment};
use flux_syntax::{LexedStr, SyntaxKind, T, TextRange, TokenOrigin};

use super::tt::{Tok, pretty};

/// A macro the compiler implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    Assert,
    Panic,
    Unreachable,
    FormatArgs,
    ConstFormatArgs,
    FormatArgsNl,
    Concat,
    Stringify,
    Line,
    Column,
    File,
    ModulePath,
    Cfg,
    CompileError,
    /// `env!`, `include_str!` and the others flux does not have yet.
    Unsupported,
}

impl Builtin {
    pub(crate) fn from_name(name: &str) -> Builtin {
        match name {
            "assert" => Builtin::Assert,
            "panic" => Builtin::Panic,
            "unreachable" => Builtin::Unreachable,
            "format_args" => Builtin::FormatArgs,
            "const_format_args" => Builtin::ConstFormatArgs,
            "format_args_nl" => Builtin::FormatArgsNl,
            "concat" => Builtin::Concat,
            "stringify" => Builtin::Stringify,
            "line" => Builtin::Line,
            "column" => Builtin::Column,
            "file" => Builtin::File,
            "module_path" => Builtin::ModulePath,
            "cfg" => Builtin::Cfg,
            "compile_error" => Builtin::CompileError,
            _ => Builtin::Unsupported,
        }
    }
}

/// What a built-in macro needs to know about its call.
pub(crate) struct BuiltinCx<'a> {
    pub name: Symbol,
    pub toks: &'a [Tok],
    /// The whole call.
    pub call: TextRange,
    /// The hygiene context of the tokens the macro makes.
    pub ctxt: u32,
    /// `::core` from a program, `crate` in `core`.
    pub core: &'static str,
    /// Where the call is, for `line!`, `column!` and `file!`: one-based
    /// line and column of its start, and the file's path.
    pub location: &'a dyn Fn() -> (u32, u32, String),
    /// The path of the module the call is in, for `module_path!`.
    pub module_path: &'a dyn Fn() -> String,
}

type Error = (TextRange, String);

impl BuiltinCx<'_> {
    fn origin(&self) -> TokenOrigin {
        TokenOrigin {
            range: self.call,
            group: TokenOrigin::SYNTHETIC,
            ctxt: self.ctxt,
        }
    }

    /// The tokens of `source`, made by the macro: `{core}` is `::core`
    /// or `crate`.
    fn quote(&self, source: &str, out: &mut Vec<Tok>) {
        self.quote_at(source, self.origin(), out);
    }

    fn quote_at(&self, source: &str, origin: TokenOrigin, out: &mut Vec<Tok>) {
        let source = source.replace("{core}", self.core);
        let lexed = LexedStr::new(&source);
        for i in 0..lexed.len() {
            let kind = lexed.kind(i);
            if kind.is_trivia() {
                continue;
            }
            let joint = i + 1 < lexed.len() && !lexed.kind(i + 1).is_trivia();
            out.push(Tok {
                kind,
                text: Symbol::intern(lexed.token_text(i)),
                joint,
                origin,
            });
        }
    }

    fn string(&self, value: &str, origin: TokenOrigin) -> Tok {
        Tok {
            kind: SyntaxKind::STRING,
            text: Symbol::intern(&format!("{value:?}")),
            joint: false,
            origin,
        }
    }

    /// The range of tokens `toks`, which the call was given.
    fn range_of(&self, toks: &[Tok]) -> TextRange {
        match toks {
            [] => self.call,
            [one] => one.range(),
            [first, .., last] => {
                if first.origin.group == last.origin.group
                    && first.origin.group != TokenOrigin::SYNTHETIC
                    && first.range().start() <= last.range().end()
                {
                    TextRange::new(first.range().start(), last.range().end())
                } else {
                    self.call
                }
            }
        }
    }

    /// The origin tokens the macro makes for `toks` get, so that what
    /// they make is where `toks` are.
    fn origin_of(&self, toks: &[Tok]) -> TokenOrigin {
        match toks {
            [first, .., last] | [first @ last] if first.origin.group == last.origin.group => {
                TokenOrigin {
                    range: self.range_of(toks),
                    group: first.origin.group,
                    ctxt: self.ctxt,
                }
            }
            _ => self.origin(),
        }
    }
}

/// The arguments of a call, each an expression, by commas; a `name =`
/// before one names it.
struct Args<'a> {
    args: Vec<Arg<'a>>,
}

/// An argument's name, with its tokens, if it has one, and its tokens.
type Arg<'a> = (Option<(Symbol, &'a [Tok])>, &'a [Tok]);

/// Splits `toks` into comma-separated expressions, as the parser reads
/// them.
fn split_exprs<'a>(cx: &BuiltinCx<'_>, toks: &'a [Tok], named: bool) -> Result<Args<'a>, Error> {
    let mut input = parser::Input::default();
    for t in toks {
        input.push(t.kind);
        if t.joint {
            input.set_last_joint();
        }
    }
    let mut args = Vec::new();
    let mut pos = 0;
    while pos < toks.len() {
        let mut name = None;
        if named
            && toks[pos].kind == SyntaxKind::IDENT
            && toks.get(pos + 1).is_some_and(|t| t.kind == T![=])
            && !toks[pos + 1].joint
        {
            name = Some((toks[pos].text, &toks[pos..pos + 1]));
            pos += 2;
        }
        let Some(n) = parser::fragment_len(&input, pos, Fragment::Expr).filter(|&n| n > 0) else {
            let at = toks.get(pos).map_or(cx.call, Tok::range);
            return Err((at, "expected expression".into()));
        };
        args.push((name, &toks[pos..pos + n]));
        pos += n;
        if pos < toks.len() {
            if toks[pos].kind != T![,] {
                return Err((toks[pos].range(), "expected `,`".into()));
            }
            pos += 1;
        }
    }
    Ok(Args { args })
}

/// Expands built-in macro `builtin`.
pub(crate) fn expand(builtin: Builtin, cx: &BuiltinCx<'_>) -> Result<Vec<Tok>, Error> {
    let mut out = Vec::new();
    match builtin {
        Builtin::Assert => assert(cx, &mut out)?,
        Builtin::Panic => panic(cx, "", &mut out)?,
        Builtin::Unreachable => panic(cx, "internal error: entered unreachable code", &mut out)?,
        Builtin::FormatArgs | Builtin::ConstFormatArgs => {
            format_args(cx, cx.toks, false, &mut out)?
        }
        Builtin::FormatArgsNl => format_args(cx, cx.toks, true, &mut out)?,
        Builtin::Concat => {
            let value = concat(cx, cx.toks)?;
            out.push(cx.string(&value, cx.origin()));
        }
        Builtin::Stringify => out.push(cx.string(&pretty(cx.toks), cx.origin())),
        Builtin::Line | Builtin::Column => {
            if !cx.toks.is_empty() {
                return Err((cx.call, format!("`{}!` takes no arguments", cx.name)));
            }
            let (line, column, _) = (cx.location)();
            let n = if builtin == Builtin::Line {
                line
            } else {
                column
            };
            cx.quote(&format!("{n}u32"), &mut out);
        }
        Builtin::File => {
            if !cx.toks.is_empty() {
                return Err((cx.call, "`file!` takes no arguments".into()));
            }
            let (_, _, path) = (cx.location)();
            out.push(cx.string(&path, cx.origin()));
        }
        Builtin::ModulePath => out.push(cx.string(&(cx.module_path)(), cx.origin())),
        Builtin::Cfg => {
            let value = cfg(cx, cx.toks)?;
            cx.quote(if value { "true" } else { "false" }, &mut out);
        }
        Builtin::CompileError => {
            let message = match cx.toks {
                [t] if t.kind == SyntaxKind::STRING => {
                    crate::literal::unescape_str(t.text.as_str()).unwrap_or_default()
                }
                _ => "compile_error! takes 1 argument".into(),
            };
            return Err((cx.call, message));
        }
        Builtin::Unsupported => {
            return Err((cx.call, format!("`{}!` is not supported yet", cx.name)));
        }
    }
    Ok(out)
}

/// `assert!(cond)` is `if !(cond) { panic("assertion failed: cond") }`;
/// with more arguments, they are `panic!`'s.
fn assert(cx: &BuiltinCx<'_>, out: &mut Vec<Tok>) -> Result<(), Error> {
    let mut input = parser::Input::default();
    for t in cx.toks {
        input.push(t.kind);
        if t.joint {
            input.set_last_joint();
        }
    }
    let n = if cx.toks.is_empty() {
        0
    } else {
        parser::fragment_len(&input, 0, Fragment::Expr).unwrap_or(0)
    };
    if n == 0 {
        return Err((
            cx.call,
            "macro requires a boolean expression as an argument".into(),
        ));
    }
    let cond = &cx.toks[..n];
    let rest = match cx.toks.get(n) {
        None => &[][..],
        Some(t) if t.kind == T![,] => &cx.toks[n + 1..],
        Some(t) => return Err((t.range(), "expected `,`".into())),
    };
    cx.quote("if ! (", out);
    out.extend_from_slice(cond);
    cx.quote(") {", out);
    if rest.is_empty() {
        cx.quote("{core}::panicking::panic(", out);
        out.push(cx.string(&format!("assertion failed: {}", pretty(cond)), cx.origin()));
        cx.quote(")", out);
    } else {
        cx.quote("{core}::panic!(", out);
        out.extend_from_slice(rest);
        cx.quote(")", out);
    }
    cx.quote("}", out);
    Ok(())
}

/// `panic!` (`prefix` empty) and `unreachable!` (`prefix` its message),
/// as rustc's `panic_2021!` and `unreachable_2021!`: a literal message
/// is passed as it is, which constant evaluation reports.
fn panic(cx: &BuiltinCx<'_>, prefix: &str, out: &mut Vec<Tok>) -> Result<(), Error> {
    if cx.toks.is_empty() {
        let message = if prefix.is_empty() {
            "explicit panic"
        } else {
            prefix
        };
        cx.quote("{core}::panicking::panic(", out);
        out.push(cx.string(message, cx.origin()));
        cx.quote(")", out);
        return Ok(());
    }
    let args = split_exprs(cx, cx.toks, true)?;
    let Some(&(None, first)) = args.args.first() else {
        return Err((cx.call, "format argument must be a string literal".into()));
    };
    // Checks the string and the arguments, as `format_args!` would.
    let mut checked = Vec::new();
    format_args(cx, cx.toks, false, &mut checked)?;
    let literal = match first {
        [t] if t.kind == SyntaxKind::STRING => Some(t),
        _ => None,
    };
    if let Some(t) = literal
        && args.args.len() == 1
    {
        let pieces = parse_format(t, cx)?;
        if pieces.placeholders.is_empty() {
            let message = if prefix.is_empty() {
                pieces.literal
            } else {
                format!("{prefix}: {}", pieces.literal)
            };
            cx.quote("{core}::panicking::panic(", out);
            out.push(cx.string(&message, cx.origin()));
            cx.quote(")", out);
            return Ok(());
        }
    }
    if prefix.is_empty()
        && let Some(t) = literal
        && t.text.as_str() == "\"{}\""
        && args.args.len() == 2
        && args.args[1].0.is_none()
    {
        // rustc's special case, for `const` panics.
        cx.quote("{ {core}::panicking::panic_display(& (", out);
        out.extend_from_slice(args.args[1].1);
        cx.quote(")); }", out);
        return Ok(());
    }
    cx.quote("{ {core}::panicking::panic_fmt(", out);
    if prefix.is_empty() {
        out.extend(checked);
    } else {
        let mut message = Vec::new();
        cx.quote(
            &format!("{{core}}::format_args!(\"{prefix}: {{}}\", {{core}}::format_args!("),
            &mut message,
        );
        out.extend(message);
        out.extend_from_slice(cx.toks);
        cx.quote("))", out);
    }
    cx.quote("); }", out);
    Ok(())
}

/// What a format string says: its text with `{{` and `}}` unescaped, for
/// one without placeholders, and its placeholders.
struct FormatString {
    literal: String,
    pieces: Vec<String>,
    placeholders: Vec<Placeholder>,
}

#[derive(Debug, Clone)]
struct Placeholder {
    arg: ArgRef,
    /// `Display` for `{}`, `Debug` for `{:?}`, ...
    trait_: &'static str,
    /// Width and precision given by arguments, which must be `usize`s.
    counts: Vec<ArgRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ArgRef {
    Next,
    Index(usize),
    Name(Symbol),
}

fn parse_format(literal: &Tok, cx: &BuiltinCx<'_>) -> Result<FormatString, Error> {
    let at = literal.range();
    let text = crate::literal::unescape_str(literal.text.as_str()).map_err(|e| (at, e))?;
    let invalid = |m: &str| (at, format!("invalid format string: {m}"));
    let mut out = FormatString {
        literal: String::new(),
        pieces: Vec::new(),
        placeholders: Vec::new(),
    };
    let mut piece = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                piece.push('{');
                out.literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                piece.push('}');
                out.literal.push('}');
            }
            '}' => return Err(invalid("unmatched `}` found")),
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') => return Err(invalid("expected `}`, found `{`")),
                        Some(c) => inner.push(c),
                        None => return Err(invalid("expected `}` but string was terminated")),
                    }
                }
                out.pieces.push(std::mem::take(&mut piece));
                out.placeholders
                    .push(placeholder(&inner).map_err(|m| invalid(&m))?);
            }
            c => {
                piece.push(c);
                out.literal.push(c);
            }
        }
    }
    out.pieces.push(piece);
    let _ = cx;
    Ok(out)
}

/// `arg:spec`, a placeholder's text between its braces.
fn placeholder(inner: &str) -> Result<Placeholder, String> {
    let (arg, spec) = match inner.find(':') {
        Some(i) => (&inner[..i], Some(&inner[i + 1..])),
        None => (inner, None),
    };
    let arg_ref = |s: &str| -> Result<ArgRef, String> {
        let s = s.trim();
        if s.is_empty() {
            Ok(ArgRef::Next)
        } else if let Ok(n) = s.parse::<usize>() {
            Ok(ArgRef::Index(n))
        } else if s
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            Ok(ArgRef::Name(Symbol::intern(s)))
        } else {
            Err(format!("invalid argument name `{s}`"))
        }
    };
    let arg = arg_ref(arg)?;
    let mut counts = Vec::new();
    let mut trait_ = "display";
    if let Some(spec) = spec {
        let chars: Vec<char> = spec.chars().collect();
        let mut i = 0;
        // fill and alignment
        if chars.len() >= 2 && matches!(chars[1], '<' | '^' | '>') {
            i = 2;
        } else if chars.first().is_some_and(|c| matches!(c, '<' | '^' | '>')) {
            i = 1;
        }
        if chars.get(i).is_some_and(|&c| c == '+' || c == '-') {
            i += 1;
        }
        if chars.get(i) == Some(&'#') {
            i += 1;
        }
        if chars.get(i) == Some(&'0') && chars.get(i + 1).is_some_and(|c| *c != '$') {
            i += 1;
        }
        // width: a number, `name$` or `n$`
        let start = i;
        while chars
            .get(i)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            i += 1;
        }
        if chars.get(i) == Some(&'$') {
            let s: String = chars[start..i].iter().collect();
            counts.push(arg_ref(&s)?);
            i += 1;
        } else if !chars[start..i].iter().all(|c| c.is_ascii_digit()) {
            // Not a width: the type follows.
            i = start;
        }
        if chars.get(i) == Some(&'.') {
            i += 1;
            if chars.get(i) == Some(&'*') {
                counts.push(ArgRef::Next);
                i += 1;
            } else {
                let start = i;
                while chars
                    .get(i)
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_')
                {
                    i += 1;
                }
                if chars.get(i) == Some(&'$') {
                    let s: String = chars[start..i].iter().collect();
                    counts.push(arg_ref(&s)?);
                    i += 1;
                } else if !chars[start..i].iter().all(|c| c.is_ascii_digit()) {
                    return Err("expected `}`, found a precision".into());
                }
            }
        }
        let ty: String = chars[i..].iter().collect();
        trait_ = match ty.trim() {
            "" => "display",
            "?" | "x?" | "X?" | "#?" => "debug",
            "x" => "lower_hex",
            "X" => "upper_hex",
            "o" => "octal",
            "b" => "binary",
            "e" => "lower_exp",
            "E" => "upper_exp",
            "p" => "pointer",
            other => return Err(format!("unknown format trait `{other}`")),
        };
    }
    // `{:.*}` takes the precision before the value.
    let arg = if counts.first() == Some(&ArgRef::Next) && arg == ArgRef::Next {
        ArgRef::Next
    } else {
        arg
    };
    Ok(Placeholder {
        arg,
        trait_,
        counts,
    })
}

/// `format_args!(fmt, args..)`: checks `fmt` against the arguments and
/// evaluates them once each, by reference, checking each implements the
/// trait its placeholders name.
fn format_args(
    cx: &BuiltinCx<'_>,
    toks: &[Tok],
    newline: bool,
    out: &mut Vec<Tok>,
) -> Result<(), Error> {
    let args = split_exprs(cx, toks, true)?;
    let Some(&(name, fmt)) = args.args.first() else {
        return Err((cx.call, "requires at least a format string argument".into()));
    };
    if name.is_some() {
        return Err((cx.call, "format argument must be a string literal".into()));
    }
    let fmt_tok = match fmt {
        [t] if t.kind == SyntaxKind::STRING => *t,
        _ => {
            return Err((
                cx.range_of(fmt),
                "format argument must be a string literal".into(),
            ));
        }
    };
    let mut format = parse_format(&fmt_tok, cx)?;
    if newline && let Some(last) = format.pieces.last_mut() {
        last.push('\n');
    }
    let positional: Vec<&[Tok]> = args.args[1..]
        .iter()
        .take_while(|(name, _)| name.is_none())
        .map(|&(_, e)| e)
        .collect();
    let mut named: Vec<(Symbol, &[Tok], &[Tok])> = Vec::new();
    for &(name, e) in &args.args[1 + positional.len()..] {
        match name {
            Some((name, name_toks)) => named.push((name, name_toks, e)),
            None => {
                return Err((
                    cx.range_of(e),
                    "positional arguments cannot follow named arguments".into(),
                ));
            }
        }
    }
    // Each argument's expression, in order: the positional ones, the
    // named ones, then the names the string captures.
    let mut exprs: Vec<Vec<Tok>> = positional.iter().map(|e| e.to_vec()).collect();
    exprs.extend(named.iter().map(|(_, _, e)| e.to_vec()));
    let mut used = vec![false; exprs.len()];
    let mut next = 0;
    let mut wanted_positional = 0;
    let mut bad_index = None;
    // Which argument each placeholder's value and counts are.
    let mut uses: Vec<(usize, &'static str)> = Vec::new();
    let mut resolve =
        |r: &ArgRef, exprs: &mut Vec<Vec<Tok>>, used: &mut Vec<bool>| -> Option<usize> {
            match r {
                ArgRef::Next => {
                    let i = next;
                    next += 1;
                    wanted_positional = wanted_positional.max(next);
                    (i < positional.len()).then_some(i)
                }
                ArgRef::Index(i) => {
                    wanted_positional = wanted_positional.max(i + 1);
                    if *i >= positional.len() {
                        bad_index.get_or_insert(*i);
                    }
                    (*i < positional.len()).then_some(*i)
                }
                ArgRef::Name(n) => {
                    if let Some(j) = named.iter().position(|(name, _, _)| name == n) {
                        return Some(positional.len() + j);
                    }
                    // A name the string captures from where it is written.
                    let captured = Tok {
                        kind: SyntaxKind::IDENT,
                        text: *n,
                        joint: false,
                        origin: TokenOrigin {
                            range: fmt_tok.range(),
                            group: fmt_tok.origin.group,
                            ctxt: fmt_tok.origin.ctxt,
                        },
                    };
                    let existing = exprs
                        .iter()
                        .skip(positional.len() + named.len())
                        .position(|e| e[0].text == *n);
                    Some(match existing {
                        Some(k) => positional.len() + named.len() + k,
                        None => {
                            exprs.push(vec![captured]);
                            used.push(false);
                            exprs.len() - 1
                        }
                    })
                }
            }
        };
    for p in &format.placeholders {
        for count in &p.counts {
            if let Some(i) = resolve(count, &mut exprs, &mut used) {
                used[i] = true;
                uses.push((i, "count"));
            }
        }
        if let Some(i) = resolve(&p.arg, &mut exprs, &mut used) {
            used[i] = true;
            uses.push((i, p.trait_));
        }
    }
    if wanted_positional > positional.len() {
        let plural = |n: usize, s: &str| format!("{n} {s}{}", if n == 1 { "" } else { "s" });
        let message = if let Some(i) = bad_index.filter(|_| next <= positional.len()) {
            format!(
                "invalid reference to positional argument {i} ({})",
                match positional.len() {
                    0 => "no arguments were given".to_owned(),
                    1 => "there is 1 argument".to_owned(),
                    n => format!("there are {n} arguments"),
                }
            )
        } else {
            format!(
                "{} in format string, but {}",
                plural(wanted_positional, "positional argument"),
                match positional.len() {
                    0 => "no arguments were given".to_owned(),
                    1 => "there is 1 argument".to_owned(),
                    n => format!("there are {n} arguments"),
                }
            )
        };
        return Err((fmt_tok.range(), message));
    }
    let unused: Vec<usize> = (0..positional.len() + named.len())
        .filter(|&i| !used[i])
        .collect();
    match unused.as_slice() {
        [] => {}
        [i] if *i >= positional.len() => {
            return Err((cx.range_of(&exprs[*i]), "named argument never used".into()));
        }
        [i] => return Err((cx.range_of(&exprs[*i]), "argument never used".into())),
        [i, ..] => {
            return Err((
                cx.range_of(&exprs[*i]),
                "multiple unused formatting arguments".into(),
            ));
        }
    }
    let mut pieces = Vec::new();
    cx.quote("& [", &mut pieces);
    for (i, piece) in format.pieces.iter().enumerate() {
        if i > 0 {
            cx.quote(",", &mut pieces);
        }
        pieces.push(cx.string(piece, cx.origin()));
    }
    cx.quote("]", &mut pieces);
    if exprs.is_empty() {
        cx.quote("{core}::fmt::Arguments::new_const(", out);
        out.extend(pieces);
        cx.quote(")", out);
        return Ok(());
    }
    cx.quote("match (", out);
    for e in &exprs {
        cx.quote("& (", out);
        out.extend_from_slice(e);
        cx.quote("),", out);
    }
    cx.quote(") { args => {", out);
    for (i, trait_) in uses {
        let origin = cx.origin_of(&exprs[i]);
        cx.quote_at(
            &format!("{{core}}::fmt::rt::{trait_}(args.{i});"),
            origin,
            out,
        );
    }
    cx.quote("{core}::fmt::Arguments::new_const(", out);
    out.extend(pieces);
    cx.quote(") } }", out);
    Ok(())
}

/// The string `concat!` makes of literals.
fn concat(cx: &BuiltinCx<'_>, toks: &[Tok]) -> Result<String, Error> {
    let args = split_exprs(cx, toks, false)?;
    let mut out = String::new();
    for (_, arg) in args.args {
        match arg {
            [t] if t.kind == SyntaxKind::STRING => {
                out.push_str(
                    &crate::literal::unescape_str(t.text.as_str()).map_err(|e| (t.range(), e))?,
                );
            }
            [t] if t.kind == SyntaxKind::CHAR => {
                out.push(
                    crate::literal::unescape_char(t.text.as_str()).map_err(|e| (t.range(), e))?,
                );
            }
            [t] if matches!(t.kind, SyntaxKind::INT_NUMBER | SyntaxKind::FLOAT_NUMBER) => {
                out.push_str(&literal_value(t.text.as_str()));
            }
            [minus, t]
                if minus.kind == T![-]
                    && matches!(t.kind, SyntaxKind::INT_NUMBER | SyntaxKind::FLOAT_NUMBER) =>
            {
                out.push('-');
                out.push_str(&literal_value(t.text.as_str()));
            }
            [t] if matches!(t.kind, T![true] | T![false]) => out.push_str(t.text.as_str()),
            [t] if matches!(t.kind, SyntaxKind::BYTE_STRING | SyntaxKind::BYTE) => {
                return Err((t.range(), "cannot concatenate a byte string literal".into()));
            }
            _ => return Err((cx.range_of(arg), "expected a literal".into())),
        }
    }
    Ok(out)
}

/// An integer literal's value, or a float literal's digits, without its
/// suffix, as `concat!` writes them.
fn literal_value(text: &str) -> String {
    if let Ok(crate::Literal::Int { value, .. }) = crate::literal::parse_int(text) {
        return value.to_string();
    }
    let end = text
        .char_indices()
        .find(|&(i, c)| c == 'f' && i > 0)
        .map_or(text.len(), |(i, _)| i);
    text[..end].to_owned()
}

/// The value of `cfg!(predicate)`, for flux's target.
fn cfg(cx: &BuiltinCx<'_>, toks: &[Tok]) -> Result<bool, Error> {
    let mut pos = 0;
    let value = cfg_predicate(cx, toks, &mut pos)?;
    if toks.get(pos).is_some_and(|t| t.kind == T![,]) {
        pos += 1;
    }
    if let Some(t) = toks.get(pos) {
        return Err((t.range(), "expected one cfg-pattern".into()));
    }
    Ok(value)
}

fn cfg_predicate(cx: &BuiltinCx<'_>, toks: &[Tok], pos: &mut usize) -> Result<bool, Error> {
    let Some(name) = toks
        .get(*pos)
        .filter(|t| t.kind == SyntaxKind::IDENT || t.kind.is_keyword())
    else {
        let at = toks.get(*pos).map_or(cx.call, Tok::range);
        return Err((at, "expected identifier".into()));
    };
    *pos += 1;
    let name = name.text.as_str();
    if matches!(name, "all" | "any" | "not") && toks.get(*pos).is_some_and(|t| t.kind == T!['(']) {
        *pos += 1;
        let mut values = Vec::new();
        while toks.get(*pos).is_some_and(|t| t.kind != T![')']) {
            values.push(cfg_predicate(cx, toks, pos)?);
            if toks.get(*pos).is_some_and(|t| t.kind == T![,]) {
                *pos += 1;
            }
        }
        *pos += 1;
        return match name {
            "all" => Ok(values.iter().all(|&v| v)),
            "any" => Ok(values.iter().any(|&v| v)),
            _ => match values.as_slice() {
                [v] => Ok(!v),
                _ => Err((cx.call, "expected 1 cfg-pattern".into())),
            },
        };
    }
    let mut value = None;
    if toks.get(*pos).is_some_and(|t| t.kind == T![=]) {
        *pos += 1;
        match toks.get(*pos) {
            Some(t) if t.kind == SyntaxKind::STRING => {
                value = Some(crate::literal::unescape_str(t.text.as_str()).unwrap_or_default());
                *pos += 1;
            }
            Some(t) => return Err((t.range(), "expected a string literal".into())),
            None => return Err((cx.call, "expected a string literal".into())),
        }
    }
    let os = std::env::consts::OS;
    Ok(match (name, value.as_deref()) {
        // flux's equivalent of rustc without `-O`: debug assertions on.
        ("debug_assertions", None) => true,
        ("panic", Some(v)) => v == "abort",
        ("unix", None) => cfg!(unix),
        ("windows", None) => cfg!(windows),
        ("target_os", Some(v)) => v == os,
        ("target_family", Some(v)) => v == std::env::consts::FAMILY,
        ("target_arch", Some(v)) => v == std::env::consts::ARCH,
        ("target_pointer_width", Some(v)) => v == "64",
        ("target_endian", Some(v)) => v == "little",
        ("target_vendor", Some(v)) => v == if os == "macos" { "apple" } else { "unknown" },
        // `test`, `feature = ".."` and names rustc does not know.
        _ => false,
    })
}
