//! Wrong rejection: a parenthesized range-to pattern, `(..=75)`, is
//! taken for a one-element tuple pattern.
//!
//! rustc accepts this and exits with 3. flux reports "mismatched types:
//! expected `u8`, found `(_,)`" at `(..=75)` (debug and -O alike).
//! Other parenthesized patterns work (`(1..=5)`, `(5..)`, `(1 | 2)`,
//! `b @ (100 | 200)`, `(b)`), and so do `..=75` unparenthesized and
//! `(..=75, _)`. rustc requires the parentheses where a range-to pattern
//! is the whole `pat` fragment of `matches!` (`matches!(x, (..=5))`), so
//! flux cannot accept that form at all.
//!
//! Found by difftest (about a third of the first 300 seeds, before
//! `--weight paren_patterns=0` kept such patterns out).
//!
//! Diagnosis: the parser, seeing `(` followed by `..`, seems to commit to
//! a tuple pattern whose first element is a rest pattern (`(.., x)`),
//! then reads `..=75` as an element, making `(..=75)` a 1-tuple.

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn f(x: u8) -> i32 {
    match x {
        (..=75) => 1,
        (76..=99) => 2,
        _ => 0,
    }
}

fn main() {
    unsafe { exit(f(5) + f(80) + f(200)) }
}
