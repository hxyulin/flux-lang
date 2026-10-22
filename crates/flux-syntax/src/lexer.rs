//! Converts source text into a flat token stream.
//!
//! The lexer is context-free and never fails: unknown characters become
//! `ERROR` tokens and malformed literals are reported as errors on otherwise
//! normal tokens. Every byte of the input belongs to exactly one token,
//! including whitespace and comments, so the tree built from these tokens
//! reproduces the source exactly.
//!
//! The result, [`LexedStr`], is a struct of arrays (kinds and offsets) with
//! no references into a tree. It can be produced on one thread and parsed
//! on another, and the parser sees only the kinds.
//!
//! The lexer works on bytes. Rust source is overwhelmingly ASCII, so every
//! decision is made on a byte first and a `char` is decoded only when a
//! byte is outside ASCII. Comments and string bodies are skipped with
//! `memchr`, which compares many bytes per instruction.

use crate::SyntaxKind::{self, *};
use crate::TextRange;

/// The tokens of a source text, trivia included.
#[derive(Debug, Clone)]
pub struct LexedStr<'a> {
    text: &'a str,
    kinds: Vec<SyntaxKind>,
    /// Byte offset of each token, plus one final entry at `text.len()`.
    starts: Vec<u32>,
    errors: Vec<LexError>,
}

/// A malformed token: an unterminated literal, an unknown character, or
/// similar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub token: usize,
    pub message: String,
}

impl<'a> LexedStr<'a> {
    pub fn new(text: &'a str) -> LexedStr<'a> {
        assert!(
            u32::try_from(text.len()).is_ok(),
            "source files are limited to 4 GiB"
        );
        // Rust averages a little over one token per four bytes, trivia
        // included; denser files would grow the arrays, copying them.
        // The tree built from them frees what is left over.
        let capacity = text.len() / 3 + 1;
        let mut lexer = Lexer {
            text,
            bytes: text.as_bytes(),
            pos: 0,
            res: LexedStr {
                text,
                kinds: Vec::with_capacity(capacity),
                starts: Vec::with_capacity(capacity + 1),
                errors: Vec::new(),
            },
        };
        while lexer.pos < text.len() {
            lexer.token();
        }
        lexer.res.starts.push(text.len() as u32);
        lexer.res
    }

    pub fn text(&self) -> &'a str {
        self.text
    }

    /// Number of tokens, excluding the end of input.
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    pub fn kind(&self, i: usize) -> SyntaxKind {
        self.kinds[i]
    }

    pub fn kinds(&self) -> &[SyntaxKind] {
        &self.kinds
    }

    pub fn token_text(&self, i: usize) -> &'a str {
        &self.text[self.starts[i] as usize..self.starts[i + 1] as usize]
    }

    pub fn token_range(&self, i: usize) -> TextRange {
        TextRange::new(self.starts[i], self.starts[i + 1])
    }

    /// Byte offset where token `i` starts; `i == len()` gives the end.
    pub fn token_start(&self, i: usize) -> u32 {
        self.starts[i]
    }

    pub fn errors(&self) -> &[LexError] {
        &self.errors
    }

    /// The token kinds, the start offsets (with a final entry at the end of
    /// the text) and the errors, for reuse by the tree builder.
    pub(crate) fn into_parts(self) -> (Vec<SyntaxKind>, Vec<u32>, Vec<LexError>) {
        (self.kinds, self.starts, self.errors)
    }
}

struct Lexer<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    res: LexedStr<'a>,
}

impl Lexer<'_> {
    /// The byte `n` ahead, or 0 past the end. Callers compare against
    /// non-zero bytes only, so the sentinel never matches by accident.
    #[inline]
    fn byte(&self, n: usize) -> u8 {
        self.bytes.get(self.pos + n).copied().unwrap_or(0)
    }

    /// The character starting at byte offset `i`, which must be a
    /// character boundary.
    #[inline]
    fn char_at(&self, i: usize) -> Option<char> {
        match *self.bytes.get(i)? {
            b if b < 0x80 => Some(b as char),
            _ => self.text[i..].chars().next(),
        }
    }

    fn push(&mut self, kind: SyntaxKind, start: usize, error: Option<String>) {
        if let Some(message) = error {
            self.res.errors.push(LexError {
                token: self.res.kinds.len(),
                message,
            });
        }
        self.res.kinds.push(kind);
        self.res.starts.push(start as u32);
    }

    fn token(&mut self) {
        let start = self.pos;
        let b = self.bytes[start];
        let mut error = None;
        let kind = match b {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C => {
                self.pos += 1;
                self.whitespace();
                WHITESPACE
            }
            b'/' if self.byte(1) == b'/' => {
                self.pos = match memchr::memchr(b'\n', &self.bytes[start..]) {
                    Some(i) => start + i,
                    None => self.bytes.len(),
                };
                COMMENT
            }
            b'/' if self.byte(1) == b'*' => {
                self.pos += 2;
                error = self.block_comment();
                COMMENT
            }
            b'\'' => {
                self.pos += 1;
                self.quote(&mut error)
            }
            b'"' => {
                self.pos += 1;
                error = self.quoted_string();
                STRING
            }
            b'r' if self.byte(1) == b'#' && self.char_at(start + 2).is_some_and(is_ident_start) => {
                // Raw identifier: `r#match`.
                self.pos += 2;
                self.ident_continue();
                IDENT
            }
            b'0'..=b'9' => {
                self.pos += 1;
                self.number(b, &mut error)
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                self.pos += 1;
                self.ident_or_prefixed_literal(start, &mut error)
            }
            0..0x80 => {
                self.pos += 1;
                match SyntaxKind::from_char(b as char) {
                    Some(kind) => kind,
                    None => {
                        error = Some(format!("unknown character {:?}", b as char));
                        ERROR
                    }
                }
            }
            _ => {
                let c = self.char_at(start).expect("token() called at end of input");
                self.pos += c.len_utf8();
                if is_whitespace(c) {
                    self.whitespace();
                    WHITESPACE
                } else if is_ident_start(c) {
                    self.ident_or_prefixed_literal(start, &mut error)
                } else {
                    error = Some(format!("unknown character {c:?}"));
                    ERROR
                }
            }
        };
        self.push(kind, start, error);
    }

    fn whitespace(&mut self) {
        loop {
            match self.byte(0) {
                b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C => self.pos += 1,
                0x80.. => match self.char_at(self.pos) {
                    Some(c) if is_whitespace(c) => self.pos += c.len_utf8(),
                    _ => return,
                },
                _ => return,
            }
        }
    }

    fn ident_continue(&mut self) {
        loop {
            let rest = &self.bytes[self.pos..];
            self.pos += rest
                .iter()
                .position(|&b| !is_ascii_ident_continue(b))
                .unwrap_or(rest.len());
            match self.char_at(self.pos) {
                Some(c) if !c.is_ascii() && is_ident_continue(c) => self.pos += c.len_utf8(),
                _ => return,
            }
        }
    }

    /// Lexes after `/*`; block comments nest.
    fn block_comment(&mut self) -> Option<String> {
        let mut depth = 1usize;
        while let Some(i) = memchr::memchr2(b'/', b'*', &self.bytes[self.pos..]) {
            self.pos += i;
            match (self.byte(0), self.byte(1)) {
                (b'/', b'*') => {
                    depth += 1;
                    self.pos += 2;
                }
                (b'*', b'/') => {
                    depth -= 1;
                    self.pos += 2;
                    if depth == 0 {
                        return None;
                    }
                }
                _ => self.pos += 1,
            }
        }
        self.pos = self.bytes.len();
        Some("unterminated block comment".to_string())
    }

    /// Lexes after `'`: a lifetime (`'a`), a char literal (`'a'`), or an
    /// unterminated char.
    fn quote(&mut self, error: &mut Option<String>) -> SyntaxKind {
        let first = self.char_at(self.pos);
        let second = first.and_then(|c| self.char_at(self.pos + c.len_utf8()));
        match (first, second) {
            // `'a'` and `'\n'` are chars; an escape is never a lifetime.
            (Some('\\'), _) | (Some(_), Some('\'')) => {
                *error = self.quoted_char();
                CHAR
            }
            (Some(c), _) if is_ident_start(c) => {
                self.pos += c.len_utf8();
                self.ident_continue();
                LIFETIME_IDENT
            }
            _ => {
                *error = self.quoted_char();
                CHAR
            }
        }
    }

    /// Lexes the body of a char or byte literal after its opening `'`.
    /// Steps over bytes: the delimiters are ASCII and never occur inside a
    /// multi-byte character.
    fn quoted_char(&mut self) -> Option<String> {
        loop {
            match self.bytes.get(self.pos) {
                None | Some(b'\n') => return Some("unterminated character literal".to_string()),
                Some(b'\'') => {
                    self.pos += 1;
                    return None;
                }
                Some(b'\\') => self.pos = (self.pos + 2).min(self.bytes.len()),
                Some(_) => self.pos += 1,
            }
        }
    }

    /// Lexes the body of a string literal after its opening quote.
    fn quoted_string(&mut self) -> Option<String> {
        while let Some(i) = memchr::memchr2(b'"', b'\\', &self.bytes[self.pos..]) {
            self.pos += i;
            if self.bytes[self.pos] == b'"' {
                self.pos += 1;
                return None;
            }
            // Skip the backslash and the byte it escapes.
            self.pos = (self.pos + 2).min(self.bytes.len());
        }
        self.pos = self.bytes.len();
        Some("unterminated string literal".to_string())
    }

    /// Lexes a raw string after its `r`: `r"..."`, `r#"..."#`.
    fn raw_string(&mut self) -> Option<String> {
        let mut hashes = 0usize;
        while self.byte(0) == b'#' {
            self.pos += 1;
            hashes += 1;
        }
        if self.byte(0) != b'"' {
            return Some("expected `\"` to start a raw string".to_string());
        }
        self.pos += 1;
        while let Some(i) = memchr::memchr(b'"', &self.bytes[self.pos..]) {
            self.pos += i + 1;
            let closing = &self.bytes[self.pos..];
            if closing.len() >= hashes && closing[..hashes].iter().all(|&b| b == b'#') {
                self.pos += hashes;
                return None;
            }
        }
        self.pos = self.bytes.len();
        Some("unterminated raw string literal".to_string())
    }

    fn ident_or_prefixed_literal(
        &mut self,
        start: usize,
        error: &mut Option<String>,
    ) -> SyntaxKind {
        self.ident_continue();
        let ident = &self.text[start..self.pos];
        let next = self.byte(0);
        let raw_next = next == b'"' || next == b'#';
        match ident {
            "b" if next == b'\'' => {
                self.pos += 1;
                *error = self.quoted_char();
                return BYTE;
            }
            "b" if next == b'"' => {
                self.pos += 1;
                *error = self.quoted_string();
                return BYTE_STRING;
            }
            "c" if next == b'"' => {
                self.pos += 1;
                *error = self.quoted_string();
                return C_STRING;
            }
            "r" if raw_next => {
                *error = self.raw_string();
                return STRING;
            }
            "br" if raw_next => {
                *error = self.raw_string();
                return BYTE_STRING;
            }
            "cr" if raw_next => {
                *error = self.raw_string();
                return C_STRING;
            }
            _ => {}
        }
        if ident == "_" {
            return UNDERSCORE;
        }
        SyntaxKind::from_keyword(ident).unwrap_or(IDENT)
    }

    /// Lexes a number after its first digit.
    fn number(&mut self, first: u8, error: &mut Option<String>) -> SyntaxKind {
        let radix = match (first, self.byte(0)) {
            (b'0', b'x') => 16,
            (b'0', b'o') => 8,
            (b'0', b'b') => 2,
            _ => 10,
        };
        if radix != 10 {
            self.pos += 1;
            let digits_start = self.pos;
            while self.byte(0) == b'_' || (self.byte(0) as char).is_digit(radix) {
                self.pos += 1;
            }
            if !self.bytes[digits_start..self.pos]
                .iter()
                .any(|&b| b != b'_')
            {
                *error = Some("missing digits after the integer base prefix".to_string());
            }
            self.ident_continue();
            return INT_NUMBER;
        }

        let mut kind = INT_NUMBER;
        let after_field_dot = after_field_dot(&self.res.kinds);
        self.decimal_digits();
        // A tuple index (`t.0.1`) never has a fraction. Otherwise `1.` is a
        // float unless the dot starts a range (`1..2`) or a field or method
        // (`1.max(2)`).
        if !after_field_dot
            && self.byte(0) == b'.'
            && !self
                .char_at(self.pos + 1)
                .is_some_and(|c| c == '.' || is_ident_start(c))
        {
            self.pos += 1;
            kind = FLOAT_NUMBER;
            self.decimal_digits();
        }
        if !after_field_dot && matches!(self.byte(0), b'e' | b'E') {
            let sign = usize::from(matches!(self.byte(1), b'+' | b'-'));
            let mut i = 1 + sign;
            while self.byte(i) == b'_' {
                i += 1;
            }
            if self.byte(i).is_ascii_digit() {
                self.pos += 1 + sign;
                self.decimal_digits();
                kind = FLOAT_NUMBER;
            }
        }
        // Suffix: `1u8`, `2.0f32`.
        self.ident_continue();
        kind
    }

    fn decimal_digits(&mut self) {
        while matches!(self.byte(0), b'0'..=b'9' | b'_') {
            self.pos += 1;
        }
    }
}

/// Rust's `Pattern_White_Space`. Narrower than `char::is_whitespace`: a
/// no-break space, for example, is not whitespace in Rust source.
fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{0085}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

/// Whether a number after the tokens `before` follows a field's `.`
/// (`x.0`) rather than a range's (`..0`), so that `x.0.1` is two tuple
/// indices rather than a float. Looks back past trivia: numbers are rare
/// enough that this is cheaper than tracking it for every token.
fn after_field_dot(before: &[SyntaxKind]) -> bool {
    let mut significant = before.iter().rev().filter(|k| !k.is_trivia());
    significant.next() == Some(&DOT) && significant.next() != Some(&DOT)
}

/// Whether `b` continues an identifier, for ASCII bytes: a table lookup
/// rather than a chain of range checks.
#[inline]
fn is_ascii_ident_continue(b: u8) -> bool {
    const TABLE: [bool; 256] = {
        let mut table = [false; 256];
        let mut b = 0;
        while b < 128 {
            table[b] = (b as u8).is_ascii_alphanumeric() || b == b'_' as usize;
            b += 1;
        }
        table
    };
    TABLE[b as usize]
}

fn is_ident_start(c: char) -> bool {
    c == '_' || unicode_ident::is_xid_start(c)
}

fn is_ident_continue(c: char) -> bool {
    unicode_ident::is_xid_continue(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(text: &str) -> String {
        let lexed = LexedStr::new(text);
        let mut out = String::new();
        for i in 0..lexed.len() {
            out.push_str(&format!("{:?} {:?}\n", lexed.kind(i), lexed.token_text(i)));
        }
        for e in lexed.errors() {
            out.push_str(&format!("error at token {}: {}\n", e.token, e.message));
        }
        out
    }

    #[track_caller]
    fn check(text: &str, expected: &str) {
        let actual = lex(text);
        assert_eq!(actual.trim(), expected.trim(), "lexing {text:?}");
    }

    #[test]
    fn punctuation_is_single_characters() {
        check(
            ">>=::",
            r#"
R_ANGLE ">"
R_ANGLE ">"
EQ "="
COLON ":"
COLON ":"
"#,
        );
    }

    #[test]
    fn numbers() {
        check(
            "1 1.5 1e3 1.5e-3f64 0xff_u8 1..2 1.max",
            r#"
INT_NUMBER "1"
WHITESPACE " "
FLOAT_NUMBER "1.5"
WHITESPACE " "
FLOAT_NUMBER "1e3"
WHITESPACE " "
FLOAT_NUMBER "1.5e-3f64"
WHITESPACE " "
INT_NUMBER "0xff_u8"
WHITESPACE " "
INT_NUMBER "1"
DOT "."
DOT "."
INT_NUMBER "2"
WHITESPACE " "
INT_NUMBER "1"
DOT "."
IDENT "max"
"#,
        );
    }

    #[test]
    fn tuple_index_is_not_a_float() {
        check(
            "t.0.1",
            r#"
IDENT "t"
DOT "."
INT_NUMBER "0"
DOT "."
INT_NUMBER "1"
"#,
        );
    }

    #[test]
    fn float_after_range_operator() {
        check(
            "..0.5",
            r#"
DOT "."
DOT "."
FLOAT_NUMBER "0.5"
"#,
        );
    }

    #[test]
    fn lifetimes_and_chars() {
        check(
            r"'a 'a' '\n' b'x' 'static",
            r#"
LIFETIME_IDENT "'a"
WHITESPACE " "
CHAR "'a'"
WHITESPACE " "
CHAR "'\\n'"
WHITESPACE " "
BYTE "b'x'"
WHITESPACE " "
LIFETIME_IDENT "'static"
"#,
        );
    }

    #[test]
    fn strings() {
        check(
            r###""a\"b" r#"x"y"# b"z" c"w" br"q""###,
            r###"
STRING "\"a\\\"b\""
WHITESPACE " "
STRING "r#\"x\"y\"#"
WHITESPACE " "
BYTE_STRING "b\"z\""
WHITESPACE " "
C_STRING "c\"w\""
WHITESPACE " "
BYTE_STRING "br\"q\""
"###,
        );
    }

    #[test]
    fn idents_and_keywords() {
        check(
            "fn r#fn _ _x Self",
            r#"
FN_KW "fn"
WHITESPACE " "
IDENT "r#fn"
WHITESPACE " "
UNDERSCORE "_"
WHITESPACE " "
IDENT "_x"
WHITESPACE " "
SELF_TYPE_KW "Self"
"#,
        );
    }

    #[test]
    fn nested_block_comment() {
        check(
            "/* a /* b */ c */x",
            r#"
COMMENT "/* a /* b */ c */"
IDENT "x"
"#,
        );
    }

    #[test]
    fn errors_do_not_stop_lexing() {
        check(
            "\"abc\n€ /* x",
            r#"
STRING "\"abc\n€ /* x"
error at token 0: unterminated string literal
"#,
        );
        check(
            "a € b",
            r#"
IDENT "a"
WHITESPACE " "
ERROR "€"
WHITESPACE " "
IDENT "b"
error at token 2: unknown character '€'
"#,
        );
    }
}
