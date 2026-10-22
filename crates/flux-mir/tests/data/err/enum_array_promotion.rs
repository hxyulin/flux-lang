// Unit variants with destructors must remain temporaries.
enum Dropped { A }
impl Drop for Dropped {
    fn drop(&mut self) {}
}
fn dropped() -> &'static [Dropped] {
    &[Dropped::A]
}

// The same match/Option path must preserve runtime provenance.
#[derive(Clone, Copy)]
enum Token { A, B }
fn runtime(token: Token, choose: bool) -> Option<&'static [Token]> {
    let parts: &[Token] = match choose {
        true => &[Token::A, token],
        false => &[Token::B],
    };
    Some(parts)
}

// Interior-mutable payloads are not constant-promotable.
use core::cell::Cell;
enum Mutable { Value(Cell<i32>) }
fn mutable() -> &'static [Mutable] {
    &[Mutable::Value(Cell::new(1))]
}

fn main() {}
