// The lifetimes of aliases inside function pointers keep their declared
// meaning: a static row stays static, and variance follows the expansion.
struct Buf<'a> {
    s: &'a str,
}

type Stream<'a> = &'a Buf<'a>;
type Pair<'a, T> = (&'a T, &'a T);
type Fixed<'a> = &'a &'static u8;

struct Holder<'a> {
    f: fn(Stream<'a>) -> bool,
}

fn static_row(f: fn(Fixed) -> u8) -> u8 {
    let x = 1u8;
    let r = &x;
    f(&r)
}

fn elided_result<'x>(f: fn(Pair<u8>) -> &u8, a: &'x u8) -> &'static u8 {
    f((a, a))
}

fn named<'a>(b: Stream<'a>, f: fn(Stream<'a>) -> &'a str) -> &'static str {
    f(b)
}

fn contravariant<'a>(h: Holder<'static>) -> Holder<'a> {
    h
}

fn main() {}
