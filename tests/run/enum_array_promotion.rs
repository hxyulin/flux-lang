//! Match-selected enum arrays retain static provenance through unsizing
//! and Option. Exercise each length, discriminant encoding, and const eval.
#[derive(Clone, Copy, Debug)]
#[repr(i16)]
enum Token {
    A = -7,
    B = 300,
    C = 301,
    End = 900,
}

const fn composite(token: Token) -> Option<&'static [Token]> {
    let parts: &[Token] = match token {
        Token::A => &[Token::A, Token::B],
        Token::B => &[Token::C],
        Token::C => &[Token::B, Token::A, Token::C],
        Token::End => return None,
    };
    Some(parts)
}

const CONSTANT: Option<&[Token]> = composite(Token::C);

fn empty() -> &'static [Token] {
    &[]
}

fn nested() -> &'static ([Token; 2], bool) {
    &([Token::C, Token::A], true)
}

fn show(value: Option<&[Token]>) {
    match value {
        Some(parts) => {
            println!("len {}", parts.len());
            for token in parts {
                println!("{:?} {}", token, *token as i16);
            }
        }
        None => println!("none"),
    }
}

fn main() {
    for token in [Token::A, Token::B, Token::C, Token::End] {
        show(composite(token));
    }
    show(CONSTANT);
    show(Some(empty()));
    let pair = nested();
    println!("{:?} {}", pair.0, pair.1);
}
