//! Trait `impl` blocks whose headers hold for some lifetimes only: a
//! lifetime named twice (`impl<'a> PartialEq for Token<'a>`, whose `Rhs`
//! is `Token<'a>` too; `impl<'a> Add for &'a V`), `'static`
//! (`impl Tr for &'static S`), and bounds between lifetimes; derives on
//! types with lifetime parameters, compared through `Option`, tuples,
//! arrays and generic code.

use core::cmp::Ordering;
use core::ops::Add;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
enum Token<'a> {
    #[default]
    End,
    Num(i64),
    Ident(&'a [u8]),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
struct Span<'a, T> {
    text: &'a [u8],
    value: T,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct V(i64, i64);

impl<'a> Add for &'a V {
    type Output = V;
    fn add(self, o: &'a V) -> V {
        V(self.0 + o.0, self.1 + o.1)
    }
}

struct Word<'a> {
    bytes: &'a [u8],
}

impl<'a> PartialEq for Word<'a> {
    fn eq(&self, o: &Word<'a>) -> bool {
        self.bytes == o.bytes
    }
}

impl<'a> From<&'a [u8]> for Word<'a> {
    fn from(bytes: &'a [u8]) -> Word<'a> {
        Word { bytes }
    }
}

trait Static {
    type Out;
    fn get(self) -> Self::Out;
}

struct S(i64);

impl Static for &'static S {
    type Out = &'static i64;
    fn get(self) -> &'static i64 {
        &self.0
    }
}

static GLOBAL: S = S(40);

fn through_bound<T: Static<Out = &'static i64>>(t: T) -> i64 {
    *t.get()
}

trait Longer {
    fn sum(&self) -> i64;
}

impl<'a, 'b: 'a> Longer for (&'a i64, &'b i64) {
    fn sum(&self) -> i64 {
        *self.0 + *self.1
    }
}

fn same<'x>(a: Token<'x>, b: Token<'x>) -> bool {
    a == b
}

fn mixed<'x, 'y>(a: Token<'x>, b: Token<'y>) -> bool {
    a == b
}

fn generic_eq<T: PartialEq>(a: T, b: T) -> bool {
    a == b
}

fn order(a: Token, b: Token) -> i64 {
    match a.cmp(&b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

fn lex(src: &[u8]) -> [Token<'_>; 3] {
    let mut out = [Token::End; 3];
    let mut n = 0;
    let mut i = 0;
    while i < src.len() && n < 3 {
        let start = i;
        if src[i].is_ascii_digit() {
            let mut v = 0;
            while i < src.len() && src[i].is_ascii_digit() {
                v = v * 10 + (src[i] - b'0') as i64;
                i += 1;
            }
            out[n] = Token::Num(v);
            n += 1;
        } else if src[i].is_ascii_alphabetic() {
            while i < src.len() && src[i].is_ascii_alphabetic() {
                i += 1;
            }
            out[n] = Token::Ident(&src[start..i]);
            n += 1;
        } else {
            i += 1;
        }
    }
    out
}

fn main() {
    let first = [b'a', b'b', b'c'];
    let second = [b'a', b'b', b'c'];
    let a = Token::Ident(&first);
    let b = Token::Ident(&second[..]);
    let c = Token::Num(3);
    show(1, (a == b) as i64);
    show(2, (a == c) as i64);
    show(3, same(a, b) as i64 + 2 * mixed(a, c) as i64);
    show(4, (a < c) as i64 + 2 * (c < a) as i64);
    show(5, order(a, b) + 10 * order(c, a) + 100 * order(Token::End, c));
    show(6, (Some(a) == Some(b)) as i64 + 2 * ((a, 1) == (b, 2)) as i64);
    show(7, ([a, c] == [b, c]) as i64 + 2 * generic_eq(a, b) as i64);
    show(8, (Token::default() == Token::End) as i64);
    let v = &V(1, 2) + &V(3, 4);
    show(9, v.0 * 10 + v.1);
    let w = Word { bytes: &first };
    let x: Word = Word::from(&second[..]);
    show(10, (w == x) as i64);
    show(11, *(&GLOBAL).get() + through_bound(&GLOBAL));
    let (p, q) = (5, 6);
    show(12, (&p, &q).sum());
    let s1 = Span { text: &first, value: 2 };
    let s2 = Span { text: &second, value: 3 };
    show(13, (s1 < s2) as i64 + 2 * (s1 == s2) as i64 + 4 * (s1.max(s2).value));
    let source = [b'x', b' ', b'4', b'2', b' ', b'y'];
    let tokens = lex(&source);
    let expect = [Token::Ident(&first[..1]), Token::Num(42), Token::End];
    show(14, (tokens[1] == expect[1]) as i64 + 2 * (tokens[0] == expect[0]) as i64);
    let mut count = 0;
    for t in tokens.iter() {
        if *t != Token::End {
            count += 1;
        }
    }
    show(15, count);
}
