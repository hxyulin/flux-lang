// Function pointers whose parameters and results are written with aliases
// that have lifetimes: each alias stands for its declared type.
struct Buf<'a> {
    s: &'a str,
}

type Stream<'a> = &'a Buf<'a>;
type Res<T> = Result<T, ()>;
type Pair<'a, T> = (&'a T, &'a T);
type Fixed<'a> = &'a &'static u8;

struct Holder<'a> {
    f: fn(Stream<'a>) -> bool,
}

fn peek(b: Stream, f: fn(Stream) -> bool) -> bool {
    f(b)
}

fn call<'a, T>(b: Stream<'a>, f: fn(Stream<'a>) -> Res<T>) -> Res<T> {
    f(b)
}

fn text(b: Stream) -> &str {
    b.s
}

fn apply<'x>(f: fn(Pair<u8>) -> &u8, a: &'x u8, b: &'x u8) -> &'x u8 {
    f((a, b))
}

fn larger(p: Pair<'_, u8>) -> &u8 {
    if p.0 > p.1 { p.0 } else { p.1 }
}

fn deref(f: fn(Fixed) -> u8) -> u8 {
    static S: u8 = 5;
    let r: &'static u8 = &S;
    f(&r)
}

fn get(x: Fixed) -> u8 {
    **x
}

fn grow<'a>(h: Holder<'a>) -> Holder<'static> {
    h
}

fn main() {
    let buf = Buf { s: "hello" };
    let _ = peek(&buf, |b| b.s.is_empty());
    let _ = call(&buf, |b| Ok(b.s.len()));
    let f: fn(Stream) -> &str = text;
    let _ = f(&buf);
    let _ = apply(larger, &3, &9);
    let _ = deref(get);
    let _ = grow(Holder { f: |b| b.s.is_empty() });
}
