// Promotion needs neither Copy nor a local 'static annotation.
enum Token { A, B, C }
fn composite(token: Token) -> Option<&'static [Token]> {
    let parts: &[Token] = match token {
        Token::A => &[Token::A, Token::B],
        Token::B => &[Token::C],
        Token::C => return None,
    };
    Some(parts)
}
const fn pair() -> &'static [Token; 2] {
    &[Token::B, Token::A]
}
const PAIR: &[Token; 2] = pair();
fn main() {
    let _ = composite(Token::A);
    let _ = PAIR;
}
