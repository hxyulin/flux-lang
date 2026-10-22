//! A JSON tokenizer and validating recursive-descent parser over `&str`,
//! with no allocation: the lexer is an `Iterator` of `Token<'a>` borrowing
//! the input, strings are checked (escapes, `\u` hex, control characters)
//! but stay borrowed, numbers follow the JSON grammar, and errors carry a
//! line and column through a custom error enum and `?`. The parser
//! computes statistics (depth, counts per kind, the longest key) and can
//! look up a value by a dotted path like `"servers.1.port"`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token<'a> {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Str(&'a str),
    Num(&'a str),
    True,
    False,
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    UnexpectedChar(char),
    UnterminatedString,
    BadEscape,
    BadNumber,
    UnexpectedToken,
    UnexpectedEnd,
    TrailingInput,
    TooDeep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Error {
    kind: ErrorKind,
    line: u32,
    col: u32,
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer { src, pos: 0 }
    }

    fn error(&self, kind: ErrorKind) -> Error {
        let before = &self.src[..self.pos];
        let line = before.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
        let col = match before.rfind('\n') {
            Some(nl) => self.pos - nl,
            None => self.pos + 1,
        } as u32;
        Error { kind, line, col }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek_byte() {
            self.pos += 1;
        }
    }

    fn keyword(&mut self, word: &str, tok: Token<'a>) -> Result<Token<'a>, Error> {
        if self.src[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(tok)
        } else {
            let c = self.src[self.pos..].chars().next().unwrap();
            Err(self.error(ErrorKind::UnexpectedChar(c)))
        }
    }

    fn string(&mut self) -> Result<Token<'a>, Error> {
        let start = self.pos + 1;
        let bytes = self.src.as_bytes();
        let mut i = start;
        loop {
            match bytes.get(i) {
                None => {
                    self.pos = i;
                    return Err(self.error(ErrorKind::UnterminatedString));
                }
                Some(b'"') => break,
                Some(b'\\') => {
                    match bytes.get(i + 1) {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => i += 2,
                        Some(b'u') => {
                            let hex = bytes.get(i + 2..i + 6).ok_or_else(|| {
                                self.pos = i;
                                self.error(ErrorKind::BadEscape)
                            })?;
                            if !hex.iter().all(u8::is_ascii_hexdigit) {
                                self.pos = i;
                                return Err(self.error(ErrorKind::BadEscape));
                            }
                            i += 6;
                        }
                        _ => {
                            self.pos = i;
                            return Err(self.error(ErrorKind::BadEscape));
                        }
                    }
                }
                Some(&b) if b < 0x20 => {
                    self.pos = i;
                    return Err(self.error(ErrorKind::UnexpectedChar(b as char)));
                }
                Some(_) => i += 1,
            }
        }
        self.pos = i + 1;
        Ok(Token::Str(&self.src[start..i]))
    }

    fn number(&mut self) -> Result<Token<'a>, Error> {
        let bytes = self.src.as_bytes();
        let start = self.pos;
        let mut i = start;
        let digits = |mut i: usize| -> usize {
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            i
        };
        if bytes[i] == b'-' {
            i += 1;
        }
        match bytes.get(i) {
            Some(b'0') => i += 1,
            Some(b'1'..=b'9') => i = digits(i),
            _ => {
                self.pos = i;
                return Err(self.error(ErrorKind::BadNumber));
            }
        }
        if bytes.get(i) == Some(&b'.') {
            let after = digits(i + 1);
            if after == i + 1 {
                self.pos = i;
                return Err(self.error(ErrorKind::BadNumber));
            }
            i = after;
        }
        if let Some(b'e' | b'E') = bytes.get(i) {
            i += 1;
            if let Some(b'+' | b'-') = bytes.get(i) {
                i += 1;
            }
            let after = digits(i);
            if after == i {
                self.pos = i;
                return Err(self.error(ErrorKind::BadNumber));
            }
            i = after;
        }
        self.pos = i;
        Ok(Token::Num(&self.src[start..i]))
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<(Token<'a>, usize), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.skip_whitespace();
        let at = self.pos;
        let b = self.peek_byte()?;
        let simple = match b {
            b'{' => Some(Token::LBrace),
            b'}' => Some(Token::RBrace),
            b'[' => Some(Token::LBracket),
            b']' => Some(Token::RBracket),
            b':' => Some(Token::Colon),
            b',' => Some(Token::Comma),
            _ => None,
        };
        let result = if let Some(tok) = simple {
            self.pos += 1;
            Ok(tok)
        } else {
            match b {
                b'"' => self.string(),
                b'-' | b'0'..=b'9' => self.number(),
                b't' => self.keyword("true", Token::True),
                b'f' => self.keyword("false", Token::False),
                b'n' => self.keyword("null", Token::Null),
                _ => {
                    let c = self.src[self.pos..].chars().next().unwrap();
                    Err(self.error(ErrorKind::UnexpectedChar(c)))
                }
            }
        };
        Some(result.map(|t| (t, at)))
    }
}

#[derive(Default, Debug)]
struct Stats<'a> {
    objects: u32,
    arrays: u32,
    strings: u32,
    numbers: u32,
    literals: u32,
    max_depth: u32,
    longest_key: &'a str,
}

const MAX_DEPTH: u32 = 32;

struct Parser<'a> {
    lexer: core::iter::Peekable<Lexer<'a>>,
    stats: Stats<'a>,
    end: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Parser { lexer: Lexer::new(src).peekable(), stats: Stats::default(), end: src.len() }
    }

    fn eof_error(&self) -> Error {
        Error { kind: ErrorKind::UnexpectedEnd, line: 0, col: self.end as u32 }
    }

    fn next_token(&mut self) -> Result<Token<'a>, Error> {
        match self.lexer.next() {
            Some(r) => r.map(|(t, _)| t),
            None => Err(self.eof_error()),
        }
    }

    fn expect(&mut self, want: Token<'a>) -> Result<(), Error> {
        let got = self.next_token()?;
        if got == want { Ok(()) } else { Err(Error { kind: ErrorKind::UnexpectedToken, line: 0, col: 0 }) }
    }

    fn value(&mut self, depth: u32) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error { kind: ErrorKind::TooDeep, line: 0, col: depth });
        }
        self.stats.max_depth = self.stats.max_depth.max(depth);
        match self.next_token()? {
            Token::LBrace => {
                self.stats.objects += 1;
                if let Some(Ok((Token::RBrace, _))) = self.lexer.peek() {
                    self.lexer.next();
                    return Ok(());
                }
                loop {
                    let Token::Str(key) = self.next_token()? else {
                        return Err(Error { kind: ErrorKind::UnexpectedToken, line: 0, col: 1 });
                    };
                    if key.len() > self.stats.longest_key.len() {
                        self.stats.longest_key = key;
                    }
                    self.expect(Token::Colon)?;
                    self.value(depth + 1)?;
                    match self.next_token()? {
                        Token::Comma => continue,
                        Token::RBrace => return Ok(()),
                        _ => return Err(Error { kind: ErrorKind::UnexpectedToken, line: 0, col: 2 }),
                    }
                }
            }
            Token::LBracket => {
                self.stats.arrays += 1;
                if matches!(self.lexer.peek(), Some(Ok((Token::RBracket, _)))) {
                    self.lexer.next();
                    return Ok(());
                }
                loop {
                    self.value(depth + 1)?;
                    match self.next_token()? {
                        Token::Comma => {}
                        Token::RBracket => return Ok(()),
                        _ => return Err(Error { kind: ErrorKind::UnexpectedToken, line: 0, col: 3 }),
                    }
                }
            }
            Token::Str(_) => self.stats.strings += 1,
            Token::Num(_) => self.stats.numbers += 1,
            Token::True | Token::False | Token::Null => self.stats.literals += 1,
            _ => return Err(Error { kind: ErrorKind::UnexpectedToken, line: 0, col: 4 }),
        }
        Ok(())
    }

    fn document(mut self) -> Result<Stats<'a>, Error> {
        self.value(1)?;
        match self.lexer.next() {
            None => Ok(self.stats),
            Some(Err(e)) => Err(e),
            Some(Ok(_)) => Err(Error { kind: ErrorKind::TrailingInput, line: 0, col: 0 }),
        }
    }
}

/// Skips one value in the token stream (assumed valid).
fn skip_value<'a>(tokens: &mut impl Iterator<Item = Token<'a>>) {
    let mut depth = 0i32;
    for t in tokens {
        match t {
            Token::LBrace | Token::LBracket => depth += 1,
            Token::RBrace | Token::RBracket => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return;
        }
    }
}

/// Finds the raw token at a dotted path (object keys and array indices).
fn lookup<'a>(src: &'a str, path: &str) -> Option<Token<'a>> {
    let mut tokens = Lexer::new(src).map_while(Result::ok).map(|(t, _)| t).peekable();
    for part in path.split('.') {
        match tokens.next()? {
            Token::LBrace => loop {
                let Token::Str(key) = tokens.next()? else { return None };
                tokens.next()?; // colon
                if key == part {
                    break;
                }
                skip_value(&mut tokens);
                if tokens.next()? != Token::Comma {
                    return None;
                }
            },
            Token::LBracket => {
                let index = part.bytes().try_fold(0usize, |n, b| {
                    b.is_ascii_digit().then(|| n * 10 + (b - b'0') as usize)
                })?;
                for _ in 0..index {
                    skip_value(&mut tokens);
                    if tokens.next()? != Token::Comma {
                        return None;
                    }
                }
            }
            _ => return None,
        }
    }
    tokens.next()
}

const DOC: &str = r#"{
  "name": "flux",
  "version": 2,
  "tags": ["compiler", "rust", "keel"],
  "servers": [
    {"host": "alpha", "port": 8080, "secure": false},
    {"host": "betaé", "port": 8443, "secure": true, "weights": [0.5, 1e3, -2.25E-2]}
  ],
  "nested": {"a": {"b": {"c": {"d": [[[]], {}]}}}},
  "escape\"d key with a long name": null
}"#;

fn print_str(s: &str) {
    unsafe { printf(c"%.*s".as_ptr(), s.len() as i32, s.as_ptr()) };
}

fn kind_name(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::UnexpectedChar(_) => "unexpected character",
        ErrorKind::UnterminatedString => "unterminated string",
        ErrorKind::BadEscape => "bad escape",
        ErrorKind::BadNumber => "bad number",
        ErrorKind::UnexpectedToken => "unexpected token",
        ErrorKind::UnexpectedEnd => "unexpected end",
        ErrorKind::TrailingInput => "trailing input",
        ErrorKind::TooDeep => "too deep",
    }
}

fn main() {
    let token_count = Lexer::new(DOC).filter(Result::is_ok).count();
    unsafe { printf(c"%zu tokens\n".as_ptr(), token_count) };

    match Parser::new(DOC).document() {
        Ok(s) => {
            unsafe {
                printf(c"valid: %u objects, %u arrays, %u strings, %u numbers, %u literals, depth %u\n".as_ptr(),
                    s.objects, s.arrays, s.strings, s.numbers, s.literals, s.max_depth);
                printf(c"longest key: ".as_ptr());
            }
            print_str(s.longest_key);
            unsafe { printf(c"\n".as_ptr()) };
        }
        Err(e) => print_str(kind_name(e.kind)),
    }

    for path in ["name", "servers.1.port", "servers.0.host", "tags.2", "nested.a.b.c", "servers.5", "version.x"] {
        print_str(path);
        print_str(" => ");
        match lookup(DOC, path) {
            Some(Token::Str(s)) => {
                print_str("\"");
                print_str(s);
                print_str("\"");
            }
            Some(Token::Num(n)) => print_str(n),
            Some(t) => print_str(if matches!(t, Token::LBrace) { "{...}" } else { "<other>" }),
            None => print_str("(none)"),
        }
        print_str("\n");
    }

    let bad = [
        "{\"a\": 1,}",
        "[1, 2",
        "{\"a\" 1}",
        "\"abc",
        "[01]",
        "[1.]",
        "{\"x\": tru}",
        "[\"bad \\q escape\"]",
        "[1] 2",
        "\n\n  @",
        "[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]",
    ];
    let mut failures = 0;
    for (i, text) in bad.iter().enumerate() {
        let err = Parser::new(text).document().unwrap_err();
        failures += 1;
        unsafe { printf(c"bad[%zu]: ".as_ptr(), i) };
        print_str(kind_name(err.kind));
        if let ErrorKind::UnexpectedChar(c) = err.kind {
            unsafe { printf(c" '%c'".as_ptr(), c as u32) };
        }
        if err.line > 0 {
            unsafe { printf(c" at %u:%u".as_ptr(), err.line, err.col) };
        }
        print_str("\n");
    }
    unsafe { exit(failures) }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}
