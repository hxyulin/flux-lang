// Function pointers written with aliases that have lifetimes, called through
// with borrowed data; the output and exit status must match rustc's.
struct Buf<'a> {
    s: &'a str,
}

type Stream<'a> = &'a Buf<'a>;
type Res<T> = Result<T, ()>;
type Pair<'a, T> = (&'a T, &'a T);
type Fixed<'a> = &'a &'static u8;

struct Parser<'a, T> {
    run: fn(Stream<'a>) -> Res<T>,
}

impl<'a, T> Parser<'a, T> {
    fn parse(&self, b: Stream<'a>) -> Res<T> {
        (self.run)(b)
    }
}

fn peek(b: Stream, f: fn(Stream) -> bool) -> bool {
    f(b)
}

fn first<'a>(b: Stream<'a>) -> Res<&'a str> {
    b.s.split(' ').next().ok_or(())
}

fn text(b: Stream) -> &str {
    b.s
}

fn is_long(b: Stream) -> bool {
    b.s.len() > 3
}

fn larger(p: Pair<'_, u8>) -> &u8 {
    if p.0 > p.1 { p.0 } else { p.1 }
}

fn apply<'x>(f: fn(Pair<u8>) -> &u8, a: &'x u8, b: &'x u8) -> &'x u8 {
    f((a, b))
}

fn count(p: Pair<&u8>) -> usize {
    (**p.0 + **p.1) as usize
}

fn get(x: Fixed) -> u8 {
    **x
}

fn main() {
    let buf = Buf { s: "hello world" };
    println!("{}", peek(&buf, is_long));
    let p = Parser { run: first };
    println!("{:?}", p.parse(&buf));
    let f: fn(Stream) -> &str = text;
    println!("{}", f(&buf));
    println!("{}", apply(larger, &3, &9));
    let x = 4u8;
    let g: fn(Pair<&u8>) -> usize = count;
    println!("{}", g((&&x, &&x)));
    static S: u8 = 5;
    let r: &'static u8 = &S;
    let h: fn(Fixed) -> u8 = get;
    std::process::exit(h(&r) as i32 + 1);
}
