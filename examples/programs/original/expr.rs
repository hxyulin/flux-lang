//! An integer expression evaluator: a tokenizer over a byte string, a
//! recursive-descent parser for `+ - * / % ( )`, unary minus and
//! variables `a`..`z` bound with `let x = expr;` statements, evaluated
//! with errors reported through `Result` and `?`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn exit(code: i32) -> !;
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Token<'a> {
    Num(i64),
    Ident(&'a [u8]),
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

#[derive(Clone, Copy, PartialEq, Debug)]
enum Error {
    UnexpectedChar(u8),
    UnexpectedToken,
    UnexpectedEnd,
    DivideByZero,
    Unbound(u8),
    TooManyTokens,
}

const MAX_TOKENS: usize = 128;

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a [u8]) -> Self {
        Lexer { src, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn next_token(&mut self) -> Result<Option<Token<'a>>, Error> {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\n' || c == b'\t' {
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
            b'+' => Token::Plus,
            b'-' => Token::Minus,
            b'*' => Token::Star,
            b'/' => Token::Slash,
            b'%' => Token::Percent,
            b'(' => Token::LParen,
            b')' => Token::RParen,
            b'=' => Token::Eq,
            b';' => Token::Semi,
            b'0'..=b'9' => {
                let mut value = (c - b'0') as i64;
                while let Some(d @ b'0'..=b'9') = self.peek() {
                    value = value * 10 + (d - b'0') as i64;
                    self.pos += 1;
                }
                Token::Num(value)
            }
            b'a'..=b'z' => {
                while let Some(b'a'..=b'z') = self.peek() {
                    self.pos += 1;
                }
                let word = &self.src[start..self.pos];
                if word == b"let" { Token::Let } else { Token::Ident(word) }
            }
            other => return Err(Error::UnexpectedChar(other)),
        };
        Ok(Some(token))
    }
}

struct Tokens<'a> {
    items: [Token<'a>; MAX_TOKENS],
    len: usize,
}

fn tokenize(src: &[u8]) -> Result<Tokens<'_>, Error> {
    let mut lexer = Lexer::new(src);
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

struct Parser<'t, 'a> {
    tokens: &'t [Token<'a>],
    pos: usize,
    vars: [Option<i64>; 26],
}

impl<'t, 'a> Parser<'t, 'a> {
    fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.pos)
    }

    fn bump(&mut self) -> Result<Token<'a>, Error> {
        let token = *self.peek().ok_or(Error::UnexpectedEnd)?;
        self.pos += 1;
        Ok(token)
    }

    fn expect(&mut self, want: Token<'a>) -> Result<(), Error> {
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
            self.vars[(name[0] - b'a') as usize] = Some(value);
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
            Token::Ident(name) => {
                let c = name[0];
                self.vars[(c - b'a') as usize].ok_or(Error::Unbound(c))
            }
            Token::LParen => {
                let value = self.expr()?;
                self.expect(Token::RParen)?;
                Ok(value)
            }
            _ => Err(Error::UnexpectedToken),
        }
    }
}

fn run(src: &[u8]) -> Result<i64, Error> {
    let tokens = tokenize(src)?;
    let mut parser = Parser { tokens: &tokens.items[..tokens.len], pos: 0, vars: [None; 26] };
    let mut last = 0;
    while parser.pos < parser.tokens.len() {
        if let Some(value) = parser.statement()? {
            last = value;
        }
    }
    Ok(last)
}

fn report(src: &[u8], result: Result<i64, Error>) {
    unsafe {
        printf(c"%.*s\n  => ".as_ptr(), src.len() as i32, src.as_ptr());
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

const PROGRAMS: [&[u8]; 8] = [
    b"1 + 2 * 3;",
    b"(1 + 2) * 3 - -4;",
    b"let x = 6; let y = 7; x * y;",
    b"let a = 100; let b = a / 7; a % 7 + b * 7;",
    b"10 / (5 - 5);",
    b"2 * (3 + 4;",
    b"let q = 1; q + z;",
    b"3 # 4;",
];

fn main() {
    let mut failures = 0;
    for src in PROGRAMS {
        let result = run(src);
        if result.is_err() {
            failures += 1;
        }
        report(src, result);
    }
    let checksum: i64 = PROGRAMS.iter().filter_map(|p| run(p).ok()).sum();
    unsafe {
        printf(c"%d failed, checksum %lld\n".as_ptr(), failures, checksum);
    }
    unsafe { exit(failures) }
}
