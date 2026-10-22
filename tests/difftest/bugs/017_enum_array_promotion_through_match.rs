//! Wrong rejection found while probing the compiler's syntax_kind.rs.
//! Its composite_parts method selects promoted arrays of token kinds
//! in a match and returns the slice through Option.
//!
//! rustc accepts this and prints `Some([A, B])` and `None`. flux rejects
//! `Some(parts)` as returning a reference to a temporary, in both modes.
//! The analogous arrays of integers are accepted. No explicit lifetime
//! is written on the local: named body-type lifetimes are a separate
//! unsupported feature in the original compiler source.

#[derive(Clone, Copy, Debug)]
enum Token { A, B }

fn composite(token: Token) -> Option<&'static [Token]> {
    let parts: &[Token] = match token {
        Token::A => &[Token::A, Token::B],
        Token::B => return None,
    };
    Some(parts)
}

fn main() {
    println!("{:?}", composite(Token::A));
    println!("{:?}", composite(Token::B));
}
