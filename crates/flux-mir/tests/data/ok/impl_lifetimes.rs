// Trait `impl` blocks whose headers hold for some lifetimes only, used
// where the lifetimes work out: a lifetime named twice, which values of
// a covariant type with different lifetimes meet at a shorter one;
// `'static` with a static; bounds between lifetimes; and bounds that
// say what associated types are with lifetimes (`I: Iterator<Item = &'a
// i32>`), in functions and `impl` blocks. Overwriting a reference does
// not touch what it borrowed (`out[n] = Token::Ident(&src[..])` in a
// loop).

use core::ops::Add;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
enum Token<'a> {
    #[default]
    End,
    Ident(&'a [u8]),
}

fn mixed<'x, 'y>(a: Token<'x>, b: Token<'y>) -> bool {
    a == b && a <= b && Some(a) == Some(b)
}

struct V(i32);

impl<'a> Add for &'a V {
    type Output = i32;
    fn add(self, o: &'a V) -> i32 {
        self.0 + o.0
    }
}

fn add_locals() -> i32 {
    let (a, b) = (V(1), V(2));
    &a + &b
}

trait Get {
    fn get(self) -> i32;
}

struct S(i32);

impl Get for &'static S {
    fn get(self) -> i32 {
        self.0
    }
}

static GLOBAL: S = S(3);

fn from_static() -> i32 {
    (&GLOBAL).get()
}

trait Sum2 {
    fn sum(&self) -> i32;
}

impl<'a, 'b: 'a> Sum2 for (&'a i32, &'b i32) {
    fn sum(&self) -> i32 {
        *self.0 + *self.1
    }
}

fn longer<'a, 'b>(x: &'a i32, y: &'b i32) -> i32 {
    (x, y).sum()
}

fn first<'a, I: Iterator<Item = &'a i32>>(mut it: I) -> Option<&'a i32> {
    it.next()
}

fn two<'a, I>(mut it: I) -> (&'a i32, &'a i32)
where
    I: Iterator<Item = &'a i32>,
{
    let a = it.next().unwrap();
    let b = it.next().unwrap();
    (a, b)
}

struct Wrap<I>(I);

impl<'a, I: Iterator<Item = &'a u8>> Iterator for Wrap<I> {
    type Item = &'a u8;
    fn next(&mut self) -> Option<&'a u8> {
        self.0.next()
    }
}

fn last<'a>(w: Wrap<core::slice::Iter<'a, u8>>) -> Option<&'a u8> {
    let mut found = None;
    for x in w {
        found = Some(x);
    }
    found
}

fn lex(src: &[u8]) -> [Token<'_>; 2] {
    let mut out = [Token::End; 2];
    let mut n = 0;
    while n < 2 {
        out[n] = Token::Ident(&src[n..n + 1]);
        n += 1;
    }
    out
}

fn main() {
    let v = [1, 2];
    let bytes = [7u8, 8];
    let (a, b) = two(v.iter());
    let t = lex(&bytes);
    let _ = mixed(t[0], Token::Ident(&bytes)) as i32
        + add_locals()
        + from_static()
        + longer(a, b)
        + *first(v.iter()).unwrap()
        + *last(Wrap(bytes.iter())).unwrap() as i32;
}
