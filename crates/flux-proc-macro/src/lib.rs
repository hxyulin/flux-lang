//! Token trees and the process protocol used by Flux's procedural macro host.
//!
//! This bootstrap API supports token construction and traversal, byte-range spans,
//! and a bounded binary process protocol. Span hygiene resolution and mixed-site
//! contexts are not supported and panic explicitly when requested. Identifiers
//! use Unicode XID classes and NFC normalization, as Rust does.

use std::fmt;
use std::str::FromStr;

mod unicode;
mod unicode_data;

std::thread_local! {
    static HOST_AVAILABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the current thread is executing a Flux procedural macro host.
pub fn is_available() -> bool {
    HOST_AVAILABLE.with(|available| available.get())
}

/// Enters the invocation context on the host runner's thread.
#[doc(hidden)]
pub fn __flux_enter_host() {
    HOST_AVAILABLE.with(|available| available.set(true));
}

/// A source byte range carried through macro expansion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    start: u32,
    end: u32,
}
impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
    pub fn call_site() -> Self {
        Self::default()
    }
    pub fn mixed_site() -> Self {
        panic!("mixed-site procedural macro spans are not supported yet")
    }
    /// The byte offset used by Flux's token protocol.
    pub fn byte_start(self) -> u32 {
        self.start
    }
    /// The byte offset used by Flux's token protocol.
    pub fn byte_end(self) -> u32 {
        self.end
    }
    /// An empty span immediately before this span.
    pub fn start(&self) -> Self {
        Self::new(self.start, self.start)
    }
    /// An empty span immediately after this span.
    pub fn end(&self) -> Self {
        Self::new(self.end, self.end)
    }
    pub fn join(self, other: Self) -> Option<Self> {
        Some(Self::new(
            self.start.min(other.start),
            self.end.max(other.end),
        ))
    }
    /// Source text is unavailable in the byte-range-only wire protocol.
    pub fn source_text(&self) -> Option<String> {
        None
    }
    pub fn resolved_at(self, _: Self) -> Self {
        panic!("procedural macro span resolution is not supported yet")
    }
    pub fn located_at(self, other: Self) -> Self {
        other
    }
}
/// Delimiters surrounding a token group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delimiter {
    Parenthesis,
    Brace,
    Bracket,
    None,
}
/// Whether a punctuation token joins the next punctuation token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spacing {
    Alone,
    Joint,
}
/// A delimited sequence of tokens.
#[derive(Clone, Debug)]
pub struct Group {
    delimiter: Delimiter,
    stream: TokenStream,
    span: Span,
}
impl Group {
    pub fn new(delimiter: Delimiter, stream: TokenStream) -> Self {
        Self {
            delimiter,
            stream,
            span: Span::call_site(),
        }
    }
    pub fn delimiter(&self) -> Delimiter {
        self.delimiter
    }
    pub fn stream(&self) -> TokenStream {
        self.stream.clone()
    }
    pub fn span(&self) -> Span {
        self.span
    }
    pub fn span_open(&self) -> Span {
        if self.span == Span::call_site() {
            self.span
        } else {
            Span::new(self.span.start, self.span.start.saturating_add(1))
        }
    }
    pub fn span_close(&self) -> Span {
        Span::new(self.span.end.saturating_sub(1), self.span.end)
    }
    pub fn set_span(&mut self, span: Span) {
        self.span = span;
    }
}
/// An identifier or keyword.
#[derive(Clone, Debug)]
pub struct Ident {
    text: String,
    span: Span,
}
impl Ident {
    pub fn new(text: &str, span: Span) -> Self {
        assert!(is_identifier(text), "invalid identifier");
        Self {
            text: unicode::normalize(text),
            span,
        }
    }
    pub fn new_raw(text: &str, span: Span) -> Self {
        assert!(
            !matches!(text, "self" | "super" | "crate" | "Self" | "_"),
            "invalid raw identifier"
        );
        let mut ident = Self::new(text, span);
        ident.text.insert_str(0, "r#");
        ident
    }
    pub fn span(&self) -> Span {
        self.span
    }
    pub fn set_span(&mut self, span: Span) {
        self.span = span;
    }
}
impl PartialEq<str> for Ident {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}
impl PartialEq<&str> for Ident {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}
/// One punctuation character.
#[derive(Clone, Debug)]
pub struct Punct {
    ch: char,
    spacing: Spacing,
    span: Span,
}
impl Punct {
    pub fn new(ch: char, spacing: Spacing) -> Self {
        assert!(is_punct(ch), "invalid punctuation");
        Self {
            ch,
            spacing,
            span: Span::call_site(),
        }
    }
    pub fn as_char(&self) -> char {
        self.ch
    }
    pub fn spacing(&self) -> Spacing {
        self.spacing
    }
    pub fn span(&self) -> Span {
        self.span
    }
    pub fn set_span(&mut self, span: Span) {
        self.span = span;
    }
}
/// A literal's spelling and source range.
#[derive(Clone, Debug)]
pub struct Literal {
    text: String,
    span: Span,
}
impl Literal {
    fn new(text: String) -> Self {
        Self {
            text,
            span: Span::call_site(),
        }
    }
    pub fn string(value: &str) -> Self {
        Self::new(format!("{value:?}"))
    }
    pub fn character(value: char) -> Self {
        Self::new(format!("{value:?}"))
    }
    /// Constructs a byte character literal, including non-ASCII byte values.
    pub fn byte_character(value: u8) -> Self {
        let mut text = String::from("b'");
        escape_byte(&mut text, value, b'\'');
        text.push('\'');
        Self::new(text)
    }
    pub fn byte_string(value: &[u8]) -> Self {
        let mut text = String::from("b\"");
        for &byte in value {
            escape_byte(&mut text, byte, b'"');
        }
        text.push('"');
        Self::new(text)
    }
    /// Constructs a C string literal without its terminating nul byte.
    pub fn c_string(value: &std::ffi::CStr) -> Self {
        let mut text = String::from("c\"");
        let mut bytes = value.to_bytes();
        while !bytes.is_empty() {
            let (valid, invalid) = match std::str::from_utf8(bytes) {
                Ok(text) => (text.len(), 0),
                Err(error) => (
                    error.valid_up_to(),
                    error
                        .error_len()
                        .unwrap_or(bytes.len() - error.valid_up_to()),
                ),
            };
            for ch in std::str::from_utf8(&bytes[..valid])
                .expect("validated UTF-8 prefix")
                .chars()
            {
                if ch == '\'' {
                    text.push(ch);
                } else {
                    text.extend(ch.escape_debug());
                }
            }
            for &byte in &bytes[valid..valid + invalid] {
                escape_byte(&mut text, byte, b'"');
            }
            bytes = &bytes[valid + invalid..];
        }
        text.push('"');
        Self::new(text)
    }
    pub fn f32_suffixed(value: f32) -> Self {
        assert!(value.is_finite(), "non-finite float literal");
        Self::new(format!("{value:?}f32"))
    }
    pub fn f32_unsuffixed(value: f32) -> Self {
        assert!(value.is_finite(), "non-finite float literal");
        Self::new(format!("{value:?}"))
    }
    pub fn f64_suffixed(value: f64) -> Self {
        assert!(value.is_finite(), "non-finite float literal");
        Self::new(format!("{value:?}f64"))
    }
    pub fn f64_unsuffixed(value: f64) -> Self {
        assert!(value.is_finite(), "non-finite float literal");
        Self::new(format!("{value:?}"))
    }
    pub fn span(&self) -> Span {
        self.span
    }
    pub fn set_span(&mut self, span: Span) {
        self.span = span;
    }
}
fn escape_byte(text: &mut String, byte: u8, quote: u8) {
    if byte == 0 {
        text.push_str("\\0");
    } else if (byte == b'\'' || byte == b'"') && byte != quote {
        text.push(byte as char);
    } else {
        text.extend(std::ascii::escape_default(byte).map(char::from));
    }
}

macro_rules! integers {
    ($($suffixed:ident $unsuffixed:ident $ty:ty),* $(,)?) => { impl Literal { $(
        pub fn $suffixed(value: $ty) -> Self { Self::new(format!("{}{}",value,stringify!($ty))) }
        pub fn $unsuffixed(value: $ty) -> Self { Self::new(value.to_string()) }
    )* } };
}
integers!(u8_suffixed u8_unsuffixed u8, u16_suffixed u16_unsuffixed u16, u32_suffixed u32_unsuffixed u32, u64_suffixed u64_unsuffixed u64, u128_suffixed u128_unsuffixed u128, usize_suffixed usize_unsuffixed usize, i8_suffixed i8_unsuffixed i8, i16_suffixed i16_unsuffixed i16, i32_suffixed i32_unsuffixed i32, i64_suffixed i64_unsuffixed i64, i128_suffixed i128_unsuffixed i128, isize_suffixed isize_unsuffixed isize);
/// One group, identifier, punctuation character, or literal.
#[derive(Clone, Debug)]
pub enum TokenTree {
    Group(Group),
    Ident(Ident),
    Punct(Punct),
    Literal(Literal),
}
impl TokenTree {
    pub fn span(&self) -> Span {
        match self {
            Self::Group(t) => t.span(),
            Self::Ident(t) => t.span(),
            Self::Punct(t) => t.span(),
            Self::Literal(t) => t.span(),
        }
    }
    pub fn set_span(&mut self, span: Span) {
        match self {
            Self::Group(t) => t.set_span(span),
            Self::Ident(t) => t.set_span(span),
            Self::Punct(t) => t.set_span(span),
            Self::Literal(t) => t.set_span(span),
        }
    }
}
macro_rules! tree_from { ($($variant:ident),*) => { $(impl From<$variant> for TokenTree { fn from(value: $variant) -> Self { Self::$variant(value) } })* }; }
tree_from!(Group, Ident, Punct, Literal);
/// An owned sequence of token trees.
#[derive(Clone, Debug, Default)]
pub struct TokenStream(Vec<TokenTree>);
impl TokenStream {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl From<TokenTree> for TokenStream {
    fn from(tree: TokenTree) -> Self {
        Self(vec![tree])
    }
}
impl IntoIterator for TokenStream {
    type Item = TokenTree;
    type IntoIter = std::vec::IntoIter<TokenTree>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
pub mod token_stream {
    pub type IntoIter = std::vec::IntoIter<super::TokenTree>;
}
impl Extend<TokenTree> for TokenStream {
    fn extend<T: IntoIterator<Item = TokenTree>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}
impl Extend<TokenStream> for TokenStream {
    fn extend<T: IntoIterator<Item = TokenStream>>(&mut self, iter: T) {
        for stream in iter {
            self.extend(stream);
        }
    }
}
impl FromIterator<TokenTree> for TokenStream {
    fn from_iter<T: IntoIterator<Item = TokenTree>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}
impl FromIterator<TokenStream> for TokenStream {
    fn from_iter<T: IntoIterator<Item = TokenStream>>(iter: T) -> Self {
        let mut out = Self::new();
        out.extend(iter);
        out
    }
}
impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}
impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}
impl fmt::Display for Punct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ch)
    }
}
impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (a, b) = match self.delimiter {
            Delimiter::Parenthesis => ("(", ")"),
            Delimiter::Brace => ("{", "}"),
            Delimiter::Bracket => ("[", "]"),
            Delimiter::None => ("", ""),
        };
        write!(f, "{a}{}{b}", self.stream)
    }
}
impl fmt::Display for TokenTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Group(v) => v.fmt(f),
            Self::Ident(v) => v.fmt(f),
            Self::Punct(v) => v.fmt(f),
            Self::Literal(v) => v.fmt(f),
        }
    }
}
impl fmt::Display for TokenStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut joint = true;
        for token in &self.0 {
            if !joint {
                f.write_str(" ")?;
            }
            token.fmt(f)?;
            joint = matches!(token,TokenTree::Punct(p) if p.spacing == Spacing::Joint);
        }
        Ok(())
    }
}
/// An invalid token stream or protocol message.
#[derive(Clone, Debug)]
pub struct LexError;
impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid procedural macro token stream")
    }
}
impl std::error::Error for LexError {}
/// Whether text is an identifier accepted by the token API, including keywords.
pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(unicode::is_start) && chars.all(unicode::is_continue)
}
/// Whether a source identifier already uses Rust's NFC spelling.
/// Raw-identifier and lifetime prefixes do not affect normalization.
pub fn is_identifier_normalized(text: &str) -> bool {
    text.is_ascii() || unicode::normalize(text) == text
}

fn is_punct(c: char) -> bool {
    ";,:.=<>!~+-*/%^&|@#?$'".contains(c)
}
impl FromStr for Literal {
    type Err = LexError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let stream: TokenStream = text.parse()?;
        if let [TokenTree::Punct(p), TokenTree::Literal(l)] = stream.0.as_slice()
            && p.ch == '-'
            && l.text.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            return Ok(Self::new(format!("-{}", l.text)));
        }
        match stream.0.as_slice() {
            [TokenTree::Literal(l)] => Ok(l.clone()),
            _ => Err(LexError),
        }
    }
}
impl FromStr for TokenStream {
    type Err = LexError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        fn call_site(stream: &mut TokenStream) {
            for token in &mut stream.0 {
                token.set_span(Span::call_site());
                if let TokenTree::Group(group) = token {
                    call_site(&mut group.stream);
                }
            }
        }
        let mut lexer = Lexer { text, pos: 0 };
        let mut stream = lexer.stream(None, 0)?;
        call_site(&mut stream);
        Ok(stream)
    }
}
struct Lexer<'a> {
    text: &'a str,
    pos: usize,
}
impl Lexer<'_> {
    fn ch(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.ch()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn suffix(&mut self) {
        if self.ch().is_some_and(unicode::is_start) {
            self.bump();
            while self.ch().is_some_and(unicode::is_continue) {
                self.bump();
            }
        }
    }

    fn number(&mut self) -> Result<(), LexError> {
        let rest = &self.text[self.pos..];
        let base = if rest.starts_with("0x") {
            16
        } else if rest.starts_with("0o") {
            8
        } else if rest.starts_with("0b") {
            2
        } else {
            10
        };
        if base != 10 {
            self.pos += 2;
            let mut digits = 0;
            while let Some(ch) = self.ch() {
                if ch == '_' {
                    self.bump();
                } else if ch.is_digit(base) {
                    digits += 1;
                    self.bump();
                } else if ch.is_ascii_digit() {
                    return Err(LexError);
                } else {
                    break;
                }
            }
            if digits == 0 {
                return Err(LexError);
            }
        } else {
            while self.ch().is_some_and(|ch| ch.is_ascii_digit() || ch == '_') {
                self.bump();
            }
            if self.ch() == Some('.')
                && !self.text[self.pos..].starts_with("..")
                && !self.text[self.pos + 1..]
                    .chars()
                    .next()
                    .is_some_and(unicode::is_start)
            {
                self.bump();
                while self.ch().is_some_and(|ch| ch.is_ascii_digit() || ch == '_') {
                    self.bump();
                }
            }
            if matches!(self.ch(), Some('e' | 'E')) {
                self.bump();
                if matches!(self.ch(), Some('+' | '-')) {
                    self.bump();
                }
                let mut digits = 0;
                while let Some(ch) = self.ch() {
                    if ch.is_ascii_digit() {
                        digits += 1;
                        self.bump();
                    } else if ch == '_' {
                        self.bump();
                    } else {
                        break;
                    }
                }
                if digits == 0 {
                    return Err(LexError);
                }
            }
        }
        self.suffix();
        Ok(())
    }

    fn quoted(&mut self, prefix: usize, quote: char) -> Result<(), LexError> {
        let byte = prefix == 1 && self.ch() == Some('b');
        let c_string = prefix == 1 && self.ch() == Some('c');
        self.pos += prefix;
        self.bump();
        let mut characters = 0;
        loop {
            let ch = self.bump().ok_or(LexError)?;
            if ch == quote {
                break;
            }
            if ch == '\\' {
                let escape = self.bump().ok_or(LexError)?;
                match escape {
                    'n' | 'r' | 't' | '\\' | '\'' | '"' => {}
                    '0' if !c_string => {}
                    'x' => {
                        let high = self.bump().and_then(|ch| ch.to_digit(16)).ok_or(LexError)?;
                        let low = self.bump().and_then(|ch| ch.to_digit(16)).ok_or(LexError)?;
                        let value = high * 16 + low;
                        if (!byte && !c_string && value > 127) || (c_string && value == 0) {
                            return Err(LexError);
                        }
                    }
                    'u' if !byte => {
                        if self.bump() != Some('{') {
                            return Err(LexError);
                        }
                        let mut value = 0u32;
                        let mut digits = 0;
                        while let Some(ch) = self.ch() {
                            if ch == '}' {
                                break;
                            }
                            self.bump();
                            if ch == '_' && digits > 0 {
                                continue;
                            }
                            value = value
                                .checked_mul(16)
                                .and_then(|value| ch.to_digit(16).map(|digit| value + digit))
                                .ok_or(LexError)?;
                            digits += 1;
                            if digits > 6 {
                                return Err(LexError);
                            }
                        }
                        if digits == 0
                            || self.bump() != Some('}')
                            || char::from_u32(value).is_none()
                            || (c_string && value == 0)
                        {
                            return Err(LexError);
                        }
                    }
                    '\n' if quote == '"' => {
                        while self
                            .ch()
                            .is_some_and(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
                        {
                            self.bump();
                        }
                        continue;
                    }
                    '\r' if quote == '"' && self.ch() == Some('\n') => {
                        self.bump();
                        while self
                            .ch()
                            .is_some_and(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
                        {
                            self.bump();
                        }
                        continue;
                    }
                    _ => return Err(LexError),
                }
            } else if (byte && !ch.is_ascii())
                || (c_string && ch == '\0')
                || (quote == '\'' && matches!(ch, '\n' | '\r' | '\t'))
            {
                return Err(LexError);
            }
            characters += 1;
        }
        if quote == '\'' && characters != 1 {
            return Err(LexError);
        }
        Ok(())
    }

    fn stream(&mut self, close: Option<char>, depth: usize) -> Result<TokenStream, LexError> {
        if depth > 128 {
            return Err(LexError);
        }
        let mut out = TokenStream::new();
        while let Some(c) = self.ch() {
            if matches!(
                c,
                '\t'..='\r' | ' ' | '\u{85}' | '\u{200E}' | '\u{200F}' | '\u{2028}' | '\u{2029}'
            ) {
                self.bump();
                continue;
            }
            if self.text[self.pos..].starts_with("///")
                && !self.text[self.pos..].starts_with("////")
                || self.text[self.pos..].starts_with("//!")
            {
                let start = self.pos;
                let inner = self.text[self.pos..].starts_with("//!");
                self.pos += 3;
                let content = self.pos;
                while self.ch().is_some_and(|v| v != '\n') {
                    self.bump();
                }
                let text = &self.text[content..self.pos];
                let bang = if inner { "!" } else { "" };
                let mut docs: TokenStream = format!("#{bang}[doc = {text:?}]").parse()?;
                for token in &mut docs.0 {
                    token.set_span(Span::new(start as u32, self.pos as u32));
                }
                out.extend(docs);
                continue;
            }
            if self.text[self.pos..].starts_with("//") {
                while self.ch().is_some_and(|v| v != '\n') {
                    self.bump();
                }
                continue;
            }
            if self.text[self.pos..].starts_with("/*") {
                let start = self.pos;
                let doc = self.text[self.pos..].starts_with("/**")
                    && !self.text[self.pos..].starts_with("/***")
                    && !self.text[self.pos..].starts_with("/**/")
                    || self.text[self.pos..].starts_with("/*!");
                let inner = self.text[self.pos..].starts_with("/*!");
                self.pos += 2;
                let mut nesting = 1;
                while nesting > 0 {
                    if self.pos == self.text.len() {
                        return Err(LexError);
                    }
                    if self.text[self.pos..].starts_with("/*") {
                        nesting += 1;
                        self.pos += 2;
                    } else if self.text[self.pos..].starts_with("*/") {
                        nesting -= 1;
                        self.pos += 2;
                    } else {
                        self.bump();
                    }
                }
                if doc {
                    let content = &self.text[start + 3..self.pos - 2];
                    let bang = if inner { "!" } else { "" };
                    let mut docs: TokenStream = format!("#{bang}[doc = {content:?}]").parse()?;
                    for token in &mut docs.0 {
                        token.set_span(Span::new(start as u32, self.pos as u32));
                    }
                    out.extend(docs);
                }
                continue;
            }
            if Some(c) == close {
                self.bump();
                return Ok(out);
            }
            if ")]}".contains(c) {
                return Err(LexError);
            }
            let start = self.pos;
            if let Some((delimiter, end)) = match c {
                '(' => Some((Delimiter::Parenthesis, ')')),
                '[' => Some((Delimiter::Bracket, ']')),
                '{' => Some((Delimiter::Brace, '}')),
                _ => None,
            } {
                self.bump();
                let stream = self.stream(Some(end), depth + 1)?;
                let mut group = Group::new(delimiter, stream);
                group.set_span(Span::new(start as u32, self.pos as u32));
                out.extend([TokenTree::Group(group)]);
                continue;
            }
            // Quoted literals, including byte and C strings, and raw strings.
            let rest = &self.text[self.pos..];
            let prefix = if rest.starts_with("br") || rest.starts_with("cr") {
                2
            } else if rest.starts_with('r') {
                1
            } else {
                0
            };
            let mut raw_end = None;
            if prefix > 0 {
                let mut at = self.pos + prefix;
                while self.text.as_bytes().get(at) == Some(&b'#') {
                    at += 1;
                }
                if self.text.as_bytes().get(at) == Some(&b'"') {
                    let hashes = at - self.pos - prefix;
                    let ending = format!("\"{}", "#".repeat(hashes));
                    let tail = &self.text[at + 1..];
                    raw_end = Some(at + 1 + tail.find(&ending).ok_or(LexError)? + ending.len());
                }
            }
            let quoted_prefix =
                if rest.starts_with("b\"") || rest.starts_with("c\"") || rest.starts_with("b'") {
                    1
                } else {
                    0
                };
            let quote = self.text[self.pos + quoted_prefix..].chars().next();
            let is_char = quote == Some('\'') && {
                let tail = &self.text[self.pos + quoted_prefix + 1..];
                let n = tail.chars().next().map_or(0, char::len_utf8);
                let mut end = 0;
                for ch in tail.chars() {
                    if unicode::is_continue(ch) {
                        end += ch.len_utf8();
                    } else {
                        break;
                    }
                }
                tail.starts_with('\\')
                    || tail.as_bytes().get(n) == Some(&b'\'')
                    || tail.as_bytes().get(end) == Some(&b'\'')
            };
            let literal = if let Some(end) = raw_end {
                self.pos = end;
                true
            } else if quote == Some('"') || is_char {
                self.quoted(quoted_prefix, quote.ok_or(LexError)?)?;
                true
            } else if c.is_ascii_digit() {
                self.number()?;
                true
            } else {
                false
            };
            let span = Span::new(start as u32, self.pos as u32);
            if literal {
                self.suffix();
                out.extend([TokenTree::Literal(Literal {
                    text: self.text[start..self.pos].into(),
                    span,
                })]);
                continue;
            }
            if unicode::is_start(c) {
                self.bump();
                if c == 'r' && self.text[self.pos..].starts_with('#') {
                    self.bump();
                }
                while self.ch().is_some_and(unicode::is_continue) {
                    self.bump();
                }
                let text = &self.text[start..self.pos];
                if let Some(raw) = text.strip_prefix("r#")
                    && (!is_identifier(raw)
                        || matches!(raw, "self" | "super" | "crate" | "Self" | "_"))
                {
                    return Err(LexError);
                }
                out.extend([TokenTree::Ident(Ident {
                    text: unicode::normalize(text),
                    span: Span::new(start as u32, self.pos as u32),
                })]);
                continue;
            }
            if is_punct(c) {
                self.bump();
                let spacing = if self
                    .ch()
                    .is_some_and(|v| is_punct(v) || c == '\'' && unicode::is_start(v))
                {
                    Spacing::Joint
                } else {
                    Spacing::Alone
                };
                out.extend([TokenTree::Punct(Punct {
                    ch: c,
                    spacing,
                    span: Span::new(start as u32, self.pos as u32),
                })]);
                continue;
            }
            return Err(LexError);
        }
        if close.is_some() {
            Err(LexError)
        } else {
            Ok(out)
        }
    }
}

impl TokenStream {
    /// Encodes token trees without losing spans, invisible groups, or joint punctuation.
    pub fn to_wire(&self) -> Vec<u8> {
        fn number(out: &mut Vec<u8>, n: u32) {
            out.extend(n.to_le_bytes());
        }
        fn stream(value: &TokenStream, out: &mut Vec<u8>) {
            number(out, value.0.len() as u32);
            for token in &value.0 {
                let span = token.span();
                number(out, span.start);
                number(out, span.end);
                match token {
                    TokenTree::Group(g) => {
                        out.push(match g.delimiter {
                            Delimiter::Parenthesis => 0,
                            Delimiter::Brace => 1,
                            Delimiter::Bracket => 2,
                            Delimiter::None => 3,
                        });
                        stream(&g.stream, out);
                    }
                    TokenTree::Ident(i) => {
                        out.push(4);
                        number(out, i.text.len() as u32);
                        out.extend(i.text.as_bytes());
                    }
                    TokenTree::Literal(l) => {
                        out.push(5);
                        number(out, l.text.len() as u32);
                        out.extend(l.text.as_bytes());
                    }
                    TokenTree::Punct(p) => {
                        out.push(6);
                        number(out, p.ch as u32);
                        out.push(u8::from(p.spacing == Spacing::Joint));
                    }
                }
            }
        }
        let mut out = b"FPM1".to_vec();
        stream(self, &mut out);
        out
    }
    /// Decodes a bounded, versioned token message.
    pub fn from_wire(bytes: &[u8]) -> Result<Self, LexError> {
        struct Reader<'a>(&'a [u8]);
        impl Reader<'_> {
            fn take(&mut self, n: usize) -> Result<&[u8], LexError> {
                if n > self.0.len() {
                    return Err(LexError);
                }
                let (a, b) = self.0.split_at(n);
                self.0 = b;
                Ok(a)
            }
            fn byte(&mut self) -> Result<u8, LexError> {
                Ok(self.take(1)?[0])
            }
            fn number(&mut self) -> Result<u32, LexError> {
                Ok(u32::from_le_bytes(
                    self.take(4)?.try_into().map_err(|_| LexError)?,
                ))
            }
            fn text(&mut self) -> Result<String, LexError> {
                let n = self.number()? as usize;
                String::from_utf8(self.take(n)?.to_vec()).map_err(|_| LexError)
            }
            fn stream(&mut self, depth: usize) -> Result<TokenStream, LexError> {
                if depth > 128 {
                    return Err(LexError);
                }
                let n = self.number()? as usize;
                if n > self.0.len() / 9 {
                    return Err(LexError);
                }
                let mut out = TokenStream::new();
                for _ in 0..n {
                    let start = self.number()?;
                    let end = self.number()?;
                    if start > end {
                        return Err(LexError);
                    }
                    let span = Span::new(start, end);
                    let tag = self.byte()?;
                    let token = match tag {
                        0..=3 => TokenTree::Group(Group {
                            delimiter: match tag {
                                0 => Delimiter::Parenthesis,
                                1 => Delimiter::Brace,
                                2 => Delimiter::Bracket,
                                _ => Delimiter::None,
                            },
                            stream: self.stream(depth + 1)?,
                            span,
                        }),
                        4 => {
                            let text = self.text()?;
                            if !is_identifier(text.strip_prefix("r#").unwrap_or(&text)) {
                                return Err(LexError);
                            }
                            TokenTree::Ident(Ident { text, span })
                        }
                        5 => TokenTree::Literal(Literal {
                            text: self.text()?,
                            span,
                        }),
                        6 => {
                            let ch = char::from_u32(self.number()?).ok_or(LexError)?;
                            if !is_punct(ch) {
                                return Err(LexError);
                            }
                            let spacing = match self.byte()? {
                                0 => Spacing::Alone,
                                1 => Spacing::Joint,
                                _ => return Err(LexError),
                            };
                            TokenTree::Punct(Punct { ch, spacing, span })
                        }
                        _ => return Err(LexError),
                    };
                    out.extend([token]);
                }
                Ok(out)
            }
        }
        if bytes.len() > 64 * 1024 * 1024 || !bytes.starts_with(b"FPM1") {
            return Err(LexError);
        }
        let mut reader = Reader(&bytes[4..]);
        let stream = reader.stream(0)?;
        if !reader.0.is_empty() {
            return Err(LexError);
        }
        Ok(stream)
    }
}

/// Writes the dependency-free token API sources used by macro host compilers.
#[cfg(feature = "host")]
pub fn write_sources(directory: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(directory)?;
    for (name, text) in [
        ("lib.rs", include_str!("lib.rs")),
        ("unicode.rs", include_str!("unicode.rs")),
        ("unicode_data.rs", include_str!("unicode_data.rs")),
    ] {
        std::fs::write(directory.join(name), text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parsed_tokens_have_call_site_spans() {
        fn check(stream: TokenStream) {
            for token in stream {
                assert_eq!(token.span(), Span::call_site());
                if let TokenTree::Group(group) = token {
                    check(group.stream());
                }
            }
        }
        check("before { nested(42) }".parse().unwrap());
    }
    #[test]
    fn identifiers_normalize_and_preserve_combining_marks() {
        let ident = Ident::new("e\u{301}", Span::call_site());
        assert_eq!(ident.to_string(), "é");
        let ident = Ident::new("a\u{327}", Span::call_site());
        assert_eq!(ident.to_string(), "a\u{327}");
        assert_eq!(
            Ident::new_raw("e\u{301}", Span::call_site()).to_string(),
            "r#é"
        );
        assert!(is_identifier("a\u{301}"));
        assert!(!is_identifier("\u{301}a"));
        assert!(!is_identifier("a²"));
    }

    #[test]
    fn span_endpoints_are_empty_spans() {
        let span = Span::new(4, 12);
        assert_eq!(span.start(), Span::new(4, 4));
        assert_eq!(span.end(), Span::new(12, 12));
        let group = Group::new(Delimiter::Brace, TokenStream::new());
        assert_eq!(group.span_open(), Span::call_site());
        assert_eq!(group.span_close(), Span::call_site());
    }

    #[test]
    fn wire_preserves_tokens() {
        let original: TokenStream = "fn r#type<'a>(x: &'a str) -> i32 { -12 + 3 }"
            .parse()
            .unwrap();
        let decoded = TokenStream::from_wire(&original.to_wire()).unwrap();
        assert_eq!(format!("{original:?}"), format!("{decoded:?}"));
        assert_eq!(original.to_string(), decoded.to_string());
    }
    #[test]
    fn raw_strings_and_character_literals() {
        let stream: TokenStream = "r###\"hello\"### b'x' '\\u{61}' br#\"raw\"#"
            .parse()
            .unwrap();
        assert_eq!(stream.into_iter().count(), 4);
    }
    #[test]
    fn malformed_streams_are_rejected() {
        for source in [
            "(",
            "}",
            "\"unterminated",
            "/* never closed",
            "2e",
            "0x",
            "0b2",
            "r#",
            "r#_",
            "r#self",
            "r##bad",
            "\"bad\\q\"",
            "'ab'",
            "b'ф'",
            "\"\\u{D800}\"",
            "a\u{A0}b",
        ] {
            assert!(source.parse::<TokenStream>().is_err(), "{source}");
        }
        let wire = "a + b".parse::<TokenStream>().unwrap().to_wire();
        for end in 0..wire.len() {
            assert!(TokenStream::from_wire(&wire[..end]).is_err());
        }
        let mut trailing = wire;
        trailing.push(0);
        assert!(TokenStream::from_wire(&trailing).is_err());
    }
}

#[cfg(test)]
mod token_tests {
    use super::*;
    #[test]
    fn docs_become_attributes() {
        let tokens: TokenStream = "/// text\n/** block */ struct S;".parse().unwrap();
        assert_eq!(
            tokens.to_string(),
            "# [doc = \" text\"] # [doc = \" block \"] struct S ;"
        );
    }
    #[test]
    fn lifetime_and_negative_literal_are_distinct_tokens() {
        let trees: Vec<_> = "'life -12"
            .parse::<TokenStream>()
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(trees.len(), 4);
        assert!(
            matches!(&trees[0],TokenTree::Punct(p) if p.as_char()=='\'' && p.spacing()==Spacing::Joint)
        );
        assert_eq!(Literal::i32_unsuffixed(-12).to_string(), "-12");
        assert_eq!("-12".parse::<Literal>().unwrap().to_string(), "-12");
    }
    #[test]
    fn invisible_groups_survive_wire() {
        let group = Group::new(Delimiter::None, "a + b".parse().unwrap());
        let stream = TokenStream::from(TokenTree::from(group));
        let decoded = TokenStream::from_wire(&stream.to_wire()).unwrap();
        assert!(
            matches!(decoded.into_iter().next(),Some(TokenTree::Group(g)) if g.delimiter()==Delimiter::None)
        );
    }
    #[test]
    fn integer_field_access_is_not_a_literal() {
        let tokens: TokenStream = "1.foo 1..2 1.25".parse().unwrap();
        assert_eq!(tokens.to_string(), "1 . foo 1 .. 2 1.25");
    }
}
