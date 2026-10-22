//! Macros the compiler implements, which `core` declares with
//! `#[rustc_builtin_macro]`: `assert!`, `panic!`, `format_args!` and
//! friends, as rustc's expand them for edition 2024.
//!
//! `format_args!` makes a `core::fmt::Arguments` as rustc's lowering
//! does: the format string's literal pieces, each argument evaluated
//! once, by reference, as an `rt::Argument` of the trait its
//! placeholders name, and the placeholders' options where some are not
//! `{}`'s.

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
            let value = crate::cfg::eval(cx.call, cx.toks)?;
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
/// one without placeholders, the literal text before each placeholder
/// and after the last, and its placeholders.
struct FormatString {
    literal: String,
    pieces: Vec<String>,
    placeholders: Vec<Placeholder>,
    /// Where each placeholder is, braces included.
    spans: Vec<TextRange>,
    /// Each argument a placeholder names, by index or name, and where.
    refs: Vec<(ArgRef, TextRange)>,
}

/// A placeholder: the argument it shows, how (`trait_`, the `rt`
/// constructor's suffix), and its options.
#[derive(Debug, Clone)]
struct Placeholder {
    arg: ArgRef,
    /// The arguments it names by index or name, and where in its text.
    refs: Vec<(ArgRef, usize, usize)>,
    /// `display` for `{}`, `debug` for `{:?}`, ...
    trait_: &'static str,
    fill: Option<char>,
    align: Option<Align>,
    /// `+` or `-`.
    sign: Option<char>,
    alternate: bool,
    zero: bool,
    /// `{:x?}` or `{:X?}`.
    debug_hex: Option<bool>,
    width: Option<Count>,
    precision: Option<Count>,
}

impl Placeholder {
    /// Whether it is `{}` or `{name}` with another trait and no options,
    /// as `new_v1` can show.
    fn is_plain(&self) -> bool {
        self.fill.is_none()
            && self.align.is_none()
            && self.sign.is_none()
            && !self.alternate
            && !self.zero
            && self.debug_hex.is_none()
            && self.width.is_none()
            && self.precision.is_none()
    }

    /// The options' bits, as `core::fmt::flags` lays them out.
    fn flags(&self) -> u32 {
        let mut flags = self.fill.unwrap_or(' ') as u32;
        match self.sign {
            Some('+') => flags |= 1 << 21,
            Some('-') => flags |= 1 << 22,
            _ => {}
        }
        if self.alternate {
            flags |= 1 << 23;
        }
        if self.zero {
            flags |= 1 << 24;
        }
        match self.debug_hex {
            Some(false) => flags |= 1 << 25,
            Some(true) => flags |= 1 << 26,
            None => {}
        }
        flags
            | match self.align {
                Some(Align::Left) => 0,
                Some(Align::Right) => 1 << 29,
                Some(Align::Center) => 2 << 29,
                None => 3 << 29,
            }
    }
}

#[derive(Debug, Clone, Copy)]
enum Align {
    Left,
    Right,
    Center,
}

/// A width or precision: a number, or an argument's value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Count {
    Is(u16),
    Arg(ArgRef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ArgRef {
    Next,
    Index(usize),
    Name(Symbol),
}

/// For each byte of the text of string literal `token` (`len` bytes),
/// and the end, its offset in the token: escapes and line
/// continuations take more of the token than of the text.
fn source_offsets(token: &str, len: usize) -> Option<Vec<u32>> {
    let quote = token.find('"')?;
    let raw = token[..quote].contains('r');
    let hashes = token[..quote].matches('#').count();
    let body_end = token.len().checked_sub(hashes + 1)?;
    let body = token.get(quote + 1..body_end)?;
    let base = quote + 1;
    let mut map = Vec::with_capacity(len + 1);
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if raw || bytes[i] != b'\\' {
            let c = body[i..].chars().next()?;
            for _ in 0..c.len_utf8() {
                map.push((base + i) as u32);
            }
            i += c.len_utf8();
            continue;
        }
        let start = i;
        match bytes.get(i + 1)? {
            b'\n' => {
                // A line continuation: the newline and the whitespace
                // after it are no text.
                i += 2;
                while i < bytes.len() && body[i..].starts_with(char::is_whitespace) {
                    i += body[i..].chars().next()?.len_utf8();
                }
                continue;
            }
            b'x' => i += 4,
            b'u' => i += body[i..].find('}')? + 1,
            _ => i += 2,
        }
        let c = crate::literal::unescape_str(&format!("\"{}\"", &body[start..i])).ok()?;
        for _ in 0..c.len() {
            map.push((base + start) as u32);
        }
    }
    map.push((base + bytes.len()) as u32);
    (map.len() == len + 1).then_some(map)
}

fn parse_format(literal: &Tok, cx: &BuiltinCx<'_>) -> Result<FormatString, Error> {
    let at = literal.range();
    let token = literal.text.as_str();
    let text = crate::literal::unescape_str(token).map_err(|e| (at, e))?;
    // Where each byte of the text is written in the literal, if the
    // literal is where its token says.
    let offsets = (at.len() as usize == token.len())
        .then(|| source_offsets(token, text.len()))
        .flatten();
    let span = |lo: usize, hi: usize| match &offsets {
        Some(map) => TextRange::new(at.start() + map[lo], at.start() + map[hi]),
        None => at,
    };
    let invalid = |m: &str| (at, format!("invalid format string: {m}"));
    let mut out = FormatString {
        literal: String::new(),
        pieces: Vec::new(),
        placeholders: Vec::new(),
        spans: Vec::new(),
        refs: Vec::new(),
    };
    let mut piece = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' if chars.peek().map(|&(_, c)| c) == Some('{') => {
                chars.next();
                piece.push('{');
                out.literal.push('{');
            }
            '}' if chars.peek().map(|&(_, c)| c) == Some('}') => {
                chars.next();
                piece.push('}');
                out.literal.push('}');
            }
            '}' => return Err(invalid("unmatched `}` found")),
            '{' => {
                let mut inner = String::new();
                let end = loop {
                    match chars.next() {
                        Some((j, '}')) => break j + 1,
                        Some((_, '{')) => return Err(invalid("expected `}`, found `{`")),
                        Some((_, c)) => inner.push(c),
                        None => return Err(invalid("expected `}` but string was terminated")),
                    }
                };
                out.pieces.push(std::mem::take(&mut piece));
                let p = placeholder(&inner).map_err(|(message, within)| match within {
                    Some((lo, hi)) => (span(i + 1 + lo, i + 1 + hi), message),
                    None => (at, format!("invalid format string: {message}")),
                })?;
                for (r, lo, hi) in &p.refs {
                    out.refs.push((r.clone(), span(i + 1 + lo, i + 1 + hi)));
                }
                out.placeholders.push(p);
                out.spans.push(span(i, end));
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

/// A placeholder's error: its message, and where in the placeholder's
/// text it is if rustc points there.
type SpecError = (String, Option<(usize, usize)>);

/// `arg:spec`, a placeholder's text between its braces, as rustc's
/// `rustc_parse_format` reads it: `[[fill]align][sign]['#']['0'][width]
/// ['.' precision][type]`.
///
/// An error is its message, and where in `inner` it is if rustc points
/// there (then it is not an "invalid format string").
fn placeholder(inner: &str) -> Result<Placeholder, SpecError> {
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
    let lead = arg.len() - arg.trim_start().len();
    let named = arg_ref(arg).map_err(|m| (m, None))?;
    let mut refs = Vec::new();
    if named != ArgRef::Next {
        refs.push((named.clone(), lead, lead + arg.trim().len()));
    }
    let mut p = Placeholder {
        refs: Vec::new(),
        arg: named,
        trait_: "display",
        fill: None,
        align: None,
        sign: None,
        alternate: false,
        zero: false,
        debug_hex: None,
        width: None,
        precision: None,
    };
    let Some(spec) = spec else {
        p.refs = refs;
        return Ok(p);
    };
    let chars: Vec<char> = spec.chars().collect();
    let spec_start = inner.len() - spec.len();
    // The byte offset in `inner` of character `k` of the spec.
    let byte = |k: usize| spec_start + chars[..k].iter().map(|c| c.len_utf8()).sum::<usize>();
    let align_of = |c: char| match c {
        '<' => Some(Align::Left),
        '>' => Some(Align::Right),
        '^' => Some(Align::Center),
        _ => None,
    };
    let mut i = 0;
    if let Some(align) = chars.get(1).and_then(|&c| align_of(c)) {
        p.fill = Some(chars[0]);
        p.align = Some(align);
        i = 2;
    } else if let Some(align) = chars.first().and_then(|&c| align_of(c)) {
        p.align = Some(align);
        i = 1;
    }
    if let Some(&c @ ('+' | '-')) = chars.get(i) {
        p.sign = Some(c);
        i += 1;
    }
    if chars.get(i) == Some(&'#') {
        p.alternate = true;
        i += 1;
    }
    if chars.get(i) == Some(&'0') && chars.get(i + 1) != Some(&'$') {
        p.zero = true;
        i += 1;
    }
    // A count: a number, or `n$` or `name$` for an argument's value.
    let mut count = |i: &mut usize| -> Result<Option<Count>, SpecError> {
        let start = *i;
        let mut end = start;
        if chars.get(start).is_some_and(|c| c.is_ascii_digit()) {
            while chars.get(end).is_some_and(|c| c.is_ascii_digit()) {
                end += 1;
            }
        } else {
            while chars
                .get(end)
                .is_some_and(|c| c.is_alphanumeric() || *c == '_')
            {
                end += 1;
            }
        }
        let s: String = chars[start..end].iter().collect();
        if chars.get(end) == Some(&'$') {
            *i = end + 1;
            let r = arg_ref(&s).map_err(|m| (m, None))?;
            refs.push((r.clone(), byte(start), byte(end)));
            return Ok(Some(Count::Arg(r)));
        }
        if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
            *i = end;
            let n: u16 = s.parse().map_err(|_| {
                (
                    format!(
                        "invalid format string: integer `{s}` does not fit into the type `u16` whose range is `0..=65535`"
                    ),
                    Some((byte(start), byte(end))),
                )
            })?;
            return Ok(Some(Count::Is(n)));
        }
        Ok(None)
    };
    p.width = count(&mut i)?;
    if chars.get(i) == Some(&'.') {
        i += 1;
        if chars.get(i) == Some(&'*') {
            p.precision = Some(Count::Arg(ArgRef::Next));
            i += 1;
        } else {
            match count(&mut i)? {
                Some(c) => p.precision = Some(c),
                None => return Err(("expected `}`, found a precision".into(), None)),
            }
        }
    }
    p.refs = refs;
    let ty: String = chars[i..].iter().collect();
    p.trait_ = match ty.trim() {
        "" => "display",
        "?" => "debug",
        "x?" => {
            p.debug_hex = Some(false);
            "debug"
        }
        "X?" => {
            p.debug_hex = Some(true);
            "debug"
        }
        "x" => "lower_hex",
        "X" => "upper_hex",
        "o" => "octal",
        "b" => "binary",
        "e" => "lower_exp",
        "E" => "upper_exp",
        "p" => "pointer",
        other => {
            let lead = ty.len() - ty.trim_start().len();
            let lo = byte(i) + lead;
            return Err((
                format!("unknown format trait `{other}`"),
                Some((lo, lo + other.len())),
            ));
        }
    };
    Ok(p)
}

/// `format_args!(fmt, args..)`: checks `fmt` against the arguments,
/// evaluates them once each, by reference, and makes the `Arguments`
/// that shows them, each with the formatting trait its placeholders
/// name (which it must implement, reported at it).
/// The text a string or integer literal shows with `{}`, as rustc's
/// `try_inline_lit` finds it, through parentheses and `&`: an integer
/// within its type (`i32` unsuffixed).
fn inlined_literal(expr: &[Tok]) -> Option<String> {
    let mut e = expr;
    loop {
        match e {
            [first, rest @ ..] if first.kind == T![&] => e = rest,
            [open, inner @ .., close] if open.kind == T!['('] && close.kind == T![')'] => e = inner,
            _ => break,
        }
    }
    let [t] = e else {
        return None;
    };
    match t.kind {
        SyntaxKind::STRING => crate::literal::unescape_str(t.text.as_str()).ok(),
        SyntaxKind::INT_NUMBER => {
            let Ok(crate::Literal::Int { value, suffix }) =
                crate::literal::parse_int(t.text.as_str())
            else {
                return None;
            };
            let max: u128 = match suffix.map(|s| s.as_str()) {
                None | Some("i32") => i32::MAX as u128,
                Some("i8") => i8::MAX as u128,
                Some("i16") => i16::MAX as u128,
                Some("i64" | "isize") => i64::MAX as u128,
                Some("i128") => i128::MAX as u128,
                Some("u8") => u8::MAX.into(),
                Some("u16") => u16::MAX.into(),
                Some("u32") => u32::MAX.into(),
                Some("u64" | "usize") => u64::MAX.into(),
                _ => u128::MAX,
            };
            (value <= max).then(|| value.to_string())
        }
        _ => None,
    }
}

/// Writes the literals plain `{}` placeholders show into the pieces
/// around them, dropping those placeholders, and the entries and
/// arguments nothing shows any more.
fn inline_literals(
    format: &mut FormatString,
    resolved: &mut Vec<(Option<usize>, Option<usize>, Option<usize>)>,
    entries: &mut Vec<(usize, &'static str)>,
    exprs: &mut Vec<Vec<Tok>>,
) {
    let texts: Vec<Option<String>> = (0..format.placeholders.len())
        .map(|i| match resolved[i] {
            (Some(e), None, None)
                if entries[e].1 == "display" && format.placeholders[i].is_plain() =>
            {
                inlined_literal(&exprs[entries[e].0])
            }
            _ => None,
        })
        .collect();
    if texts.iter().all(Option::is_none) {
        return;
    }
    let mut pieces = Vec::new();
    let mut placeholders = Vec::new();
    let mut kept = Vec::new();
    let mut current = format.pieces[0].clone();
    for (i, text) in texts.iter().enumerate() {
        let after = format.pieces.get(i + 1).cloned().unwrap_or_default();
        match text {
            Some(text) => {
                current.push_str(text);
                current.push_str(&after);
            }
            None => {
                pieces.push(std::mem::replace(&mut current, after));
                placeholders.push(format.placeholders[i].clone());
                kept.push(resolved[i]);
            }
        }
    }
    pieces.push(current);
    // The entries the placeholders left need, in the order they first
    // need them, and the arguments those show.
    let mut new_entries: Vec<(usize, &'static str)> = Vec::new();
    let remap = |e: Option<usize>, new_entries: &mut Vec<(usize, &'static str)>| {
        e.map(|e| {
            let entry = entries[e];
            match new_entries.iter().position(|&n| n == entry) {
                Some(i) => i,
                None => {
                    new_entries.push(entry);
                    new_entries.len() - 1
                }
            }
        })
    };
    let resolved_new: Vec<_> = kept
        .iter()
        .map(|&(v, w, p)| {
            let w = remap(w, &mut new_entries);
            let p = remap(p, &mut new_entries);
            let v = remap(v, &mut new_entries);
            (v, w, p)
        })
        .collect();
    let mut args: Vec<usize> = Vec::new();
    for entry in new_entries.iter_mut() {
        entry.0 = match args.iter().position(|&a| a == entry.0) {
            Some(i) => i,
            None => {
                args.push(entry.0);
                args.len() - 1
            }
        };
    }
    *exprs = args.iter().map(|&a| exprs[a].clone()).collect();
    *entries = new_entries;
    *resolved = resolved_new;
    format.pieces = pieces;
    format.placeholders = placeholders;
}

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
                            range: format
                                .refs
                                .iter()
                                .find(|(r, _)| r == &ArgRef::Name(*n))
                                .map_or(fmt_tok.range(), |&(_, at)| at),
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
    // The `rt::Argument`s: each argument with how it is shown, once per
    // way, in the order placeholders first need them; and for each
    // placeholder, its value's, width's and precision's.
    let mut entries: Vec<(usize, &'static str)> = Vec::new();
    let mut entry = |arg: usize, how: &'static str| -> usize {
        match entries.iter().position(|&e| e == (arg, how)) {
            Some(i) => i,
            None => {
                entries.push((arg, how));
                entries.len() - 1
            }
        }
    };
    let mut resolved: Vec<(Option<usize>, Option<usize>, Option<usize>)> = Vec::new();
    for p in &format.placeholders {
        // rustc reads the width, then the precision (`.*` takes the next
        // argument), then the value.
        let mut count =
            |c: &Option<Count>, exprs: &mut Vec<Vec<Tok>>, used: &mut Vec<bool>| -> Option<usize> {
                match c {
                    Some(Count::Arg(r)) => {
                        let i = resolve(r, exprs, used)?;
                        used[i] = true;
                        Some(entry(i, "count"))
                    }
                    _ => None,
                }
            };
        let width = count(&p.width, &mut exprs, &mut used);
        let precision = count(&p.precision, &mut exprs, &mut used);
        let value = resolve(&p.arg, &mut exprs, &mut used).map(|i| {
            used[i] = true;
            entry(i, p.trait_)
        });
        resolved.push((value, width, precision));
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
        let at = match bad_index.filter(|_| next <= positional.len()) {
            Some(i) => format
                .refs
                .iter()
                .find(|(r, _)| r == &ArgRef::Index(i))
                .map(|&(_, at)| at),
            None => format.spans.first().copied(),
        };
        return Err((at.unwrap_or(fmt_tok.range()), message));
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
    // rustc writes literals that plain `{}`s show into the string
    // (`inline_literals`): `format!("{}", 42)` is `"42"`.
    inline_literals(&mut format, &mut resolved, &mut entries, &mut exprs);
    // The pieces: the text before each placeholder, and after the last
    // if there is any.
    if format.pieces.last().is_some_and(|p| p.is_empty()) {
        format.pieces.pop();
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
    // `new_v1` where each placeholder is `{}`-like and shows the entry
    // of its position, as rustc's lowering decides.
    let plain = format.placeholders.len() == entries.len()
        && format
            .placeholders
            .iter()
            .zip(&resolved)
            .enumerate()
            .all(|(i, (p, r))| p.is_plain() && r.0 == Some(i));
    if plain {
        cx.quote("{core}::fmt::Arguments::new_v1(", out);
    } else {
        cx.quote("{core}::fmt::Arguments::new_v1_formatted(", out);
    }
    out.extend(pieces);
    cx.quote(", & match (", out);
    for e in &exprs {
        cx.quote("& (", out);
        out.extend_from_slice(e);
        cx.quote("),", out);
    }
    cx.quote(") { args => [", out);
    for &(i, how) in &entries {
        let origin = cx.origin_of(&exprs[i]);
        let constructor = match how {
            "count" => "from_usize".to_owned(),
            how => format!("new_{how}"),
        };
        cx.quote_at(
            &format!("{{core}}::fmt::rt::Argument::{constructor}(args.{i}),"),
            origin,
            out,
        );
    }
    cx.quote("] }", out);
    if !plain {
        cx.quote(", & [", out);
        let count = |c: &Option<Count>, arg: Option<usize>| match (c, arg) {
            (Some(Count::Is(n)), _) => format!("{{core}}::fmt::rt::Count::Is({n}u16)"),
            (Some(Count::Arg(_)), Some(i)) => format!("{{core}}::fmt::rt::Count::Param({i}usize)"),
            _ => "{core}::fmt::rt::Count::Implied".to_owned(),
        };
        for (p, &(value, width, precision)) in format.placeholders.iter().zip(&resolved) {
            cx.quote(
                &format!(
                    "{{core}}::fmt::rt::Placeholder::new({}usize, {}u32, {}, {}),",
                    value.unwrap_or(0),
                    p.flags(),
                    count(&p.precision, precision),
                    count(&p.width, width),
                ),
                out,
            );
        }
        cx.quote("]", out);
    }
    cx.quote(")", out);
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
