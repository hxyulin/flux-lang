//! An integer expression evaluator: reads lines from stdin, tokenizes
//! each, and parses it with a recursive-descent parser for
//! `+ - * / % ( )`, unary minus and variables `a`..`z` bound with
//! `let x = expr;` statements, evaluated with errors reported through
//! `Result` and `?`. Exits with the number of lines that failed.
//!
//! Rewritten for flux from `original/expr.rs`: no byte literals or
//! byte strings (character constants instead, input from stdin), no
//! range slicing (tokens and identifiers as index ranges), no derives on
//! a lifetime-generic enum (identifiers hold their letter), no `Debug`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn putchar(c: i32) -> i32;
    fn getchar() -> i32;
    fn exit(code: i32) -> !;
}

const SPACE: u8 = 32;
const TAB: u8 = 9;
const PLUS: u8 = 43;
const MINUS: u8 = 45;
const STAR: u8 = 42;
const SLASH: u8 = 47;
const PERCENT: u8 = 37;
const LPAREN: u8 = 40;
const RPAREN: u8 = 41;
const EQUALS: u8 = 61;
const SEMI: u8 = 59;
const DIGIT_0: u8 = 48;
const DIGIT_9: u8 = 57;
const LOWER_A: u8 = 97;
const LOWER_Z: u8 = 122;
const NEWLINE: i32 = 10;

#[derive(Clone, Copy, PartialEq)]
enum Token {
    Num(i64),
    Ident(u8),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    Eq,
    Semi,
    Let,
}

#[derive(Clone, Copy, PartialEq)]
enum Error {
    UnexpectedChar(u8),
    UnexpectedToken,
    UnexpectedEnd,
    DivideByZero,
    Unbound(u8),
    TooManyTokens,
}

const MAX_TOKENS: usize = 128;
const MAX_LINE: usize = 256;

/// Lexes `src[..end]`.
struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a [u8], end: usize) -> Self {
        Lexer { src, pos: 0, end }
    }

    fn peek(&self) -> Option<u8> {
        if self.pos < self.end { self.src.get(self.pos).map(|c| *c) } else { None }
    }

    fn next_token(&mut self) -> Result<Option<Token>, Error> {
        while let Some(c) = self.peek() {
            if c == SPACE || c == TAB {
                self.pos += 1;
            } else {
                break;
            }
        }
        let Some(c) = self.peek() else {
            return Ok(None);
        };
        let start = self.pos;
        self.pos += 1;
        let token = match c {
            PLUS => Token::Plus,
            MINUS => Token::Minus,
            STAR => Token::Star,
            SLASH => Token::Slash,
            PERCENT => Token::Percent,
            LPAREN => Token::LParen,
            RPAREN => Token::RParen,
            EQUALS => Token::Eq,
            SEMI => Token::Semi,
            DIGIT_0..=DIGIT_9 => {
                let mut value = (c - DIGIT_0) as i64;
                while let Some(d @ DIGIT_0..=DIGIT_9) = self.peek() {
                    value = value * 10 + (d - DIGIT_0) as i64;
                    self.pos += 1;
                }
                Token::Num(value)
            }
            LOWER_A..=LOWER_Z => {
                while let Some(LOWER_A..=LOWER_Z) = self.peek() {
                    self.pos += 1;
                }
                let s = self.src;
                // "let"
                if self.pos - start == 3 && s[start] == 108 && s[start + 1] == 101 && s[start + 2] == 116 {
                    Token::Let
                } else {
                    Token::Ident(c)
                }
            }
            other => return Err(Error::UnexpectedChar(other)),
        };
        Ok(Some(token))
    }
}

struct Tokens {
    items: [Token; MAX_TOKENS],
    len: usize,
}

fn tokenize(src: &[u8], len: usize) -> Result<Tokens, Error> {
    let mut lexer = Lexer::new(src, len);
    let mut tokens = Tokens { items: [Token::Semi; MAX_TOKENS], len: 0 };
    while let Some(token) = lexer.next_token()? {
        if tokens.len == MAX_TOKENS {
            return Err(Error::TooManyTokens);
        }
        tokens.items[tokens.len] = token;
        tokens.len += 1;
    }
    Ok(tokens)
}

struct Parser<'t> {
    tokens: &'t [Token],
    len: usize,
    pos: usize,
    vars: [Option<i64>; 26],
}

impl<'t> Parser<'t> {
    fn peek(&self) -> Option<&Token> {
        if self.pos < self.len { self.tokens.get(self.pos) } else { None }
    }

    fn bump(&mut self) -> Result<Token, Error> {
        let token = *self.peek().ok_or(Error::UnexpectedEnd)?;
        self.pos += 1;
        Ok(token)
    }

    fn expect(&mut self, want: Token) -> Result<(), Error> {
        if self.bump()? == want { Ok(()) } else { Err(Error::UnexpectedToken) }
    }

    /// statement := 'let' ident '=' expr ';' | expr ';'
    fn statement(&mut self) -> Result<Option<i64>, Error> {
        if let Some(Token::Let) = self.peek() {
            self.pos += 1;
            let Token::Ident(name) = self.bump()? else {
                return Err(Error::UnexpectedToken);
            };
            self.expect(Token::Eq)?;
            let value = self.expr()?;
            self.expect(Token::Semi)?;
            self.vars[(name - LOWER_A) as usize] = Some(value);
            Ok(None)
        } else {
            let value = self.expr()?;
            self.expect(Token::Semi)?;
            Ok(Some(value))
        }
    }

    /// expr := term (('+' | '-') term)*
    fn expr(&mut self) -> Result<i64, Error> {
        let mut value = self.term()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => {
                    self.pos += 1;
                    value += self.term()?;
                }
                Some(Token::Minus) => {
                    self.pos += 1;
                    value -= self.term()?;
                }
                _ => return Ok(value),
            }
        }
    }

    /// term := unary (('*' | '/' | '%') unary)*
    fn term(&mut self) -> Result<i64, Error> {
        let mut value = self.unary()?;
        while let Some(&op @ (Token::Star | Token::Slash | Token::Percent)) = self.peek() {
            self.pos += 1;
            let rhs = self.unary()?;
            value = match op {
                Token::Star => value * rhs,
                _ if rhs == 0 => return Err(Error::DivideByZero),
                Token::Slash => value / rhs,
                _ => value % rhs,
            };
        }
        Ok(value)
    }

    /// unary := '-' unary | atom
    fn unary(&mut self) -> Result<i64, Error> {
        if let Some(Token::Minus) = self.peek() {
            self.pos += 1;
            return Ok(-self.unary()?);
        }
        self.atom()
    }

    /// atom := number | ident | '(' expr ')'
    fn atom(&mut self) -> Result<i64, Error> {
        match self.bump()? {
            Token::Num(n) => Ok(n),
            Token::Ident(c) => self.vars[(c - LOWER_A) as usize].ok_or(Error::Unbound(c)),
            Token::LParen => {
                let value = self.expr()?;
                self.expect(Token::RParen)?;
                Ok(value)
            }
            _ => Err(Error::UnexpectedToken),
        }
    }
}

fn run(src: &[u8], len: usize) -> Result<i64, Error> {
    let tokens = tokenize(src, len)?;
    let mut parser = Parser { tokens: &tokens.items, len: tokens.len, pos: 0, vars: [None; 26] };
    let mut last = 0;
    while parser.pos < parser.len {
        if let Some(value) = parser.statement()? {
            last = value;
        }
    }
    Ok(last)
}

fn report(line: &[u8], len: usize, result: Result<i64, Error>) {
    for i in 0..len {
        unsafe { putchar(line[i] as i32) };
    }
    unsafe {
        printf(c"\n  => ".as_ptr());
        match result {
            Ok(v) => printf(c"%lld\n".as_ptr(), v),
            Err(Error::UnexpectedChar(c)) => printf(c"error: unexpected character '%c'\n".as_ptr(), c as i32),
            Err(Error::UnexpectedToken) => printf(c"error: unexpected token\n".as_ptr()),
            Err(Error::UnexpectedEnd) => printf(c"error: unexpected end of input\n".as_ptr()),
            Err(Error::DivideByZero) => printf(c"error: division by zero\n".as_ptr()),
            Err(Error::Unbound(c)) => printf(c"error: unbound variable '%c'\n".as_ptr(), c as i32),
            Err(Error::TooManyTokens) => printf(c"error: too many tokens\n".as_ptr()),
        };
    }
}

/// Reads one line into `buf`, without its newline; `None` at end of input.
fn read_line(buf: &mut [u8]) -> Option<usize> {
    let mut len = 0;
    loop {
        let c = unsafe { getchar() };
        if c < 0 {
            return if len == 0 { None } else { Some(len) };
        }
        if c == NEWLINE {
            return Some(len);
        }
        if len < buf.len() {
            buf[len] = c as u8;
            len += 1;
        }
    }
}

fn main() {
    let mut failures = 0;
    let mut checksum = 0i64;
    let mut line = [0u8; MAX_LINE];
    while let Some(len) = read_line(&mut line) {
        if len == 0 {
            continue;
        }
        let result = run(&line, len);
        match result {
            Ok(v) => checksum += v,
            Err(_) => failures += 1,
        }
        report(&line, len, result);
    }
    unsafe {
        printf(c"%d failed, checksum %lld\n".as_ptr(), failures, checksum);
        exit(failures)
    }
}
