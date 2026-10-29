trait Input {
    type Token;
    type Slice;
    fn token(self) -> Self::Token;
    fn slice(self) -> Self::Slice;
}
struct Bytes<'a>(&'a [u8]);
impl<'a> Input for Bytes<'a> {
    type Token = u8;
    type Slice = &'a [u8];
    fn token(self) -> u8 {
        self.0[0]
    }
    fn slice(self) -> Self::Slice {
        self.0
    }
}
struct Wide;
impl Input for Wide {
    type Token = u16;
    type Slice = &'static [u8];
    fn token(self) -> u16 {
        7
    }
    fn slice(self) -> Self::Slice {
        &[7]
    }
}
fn tupled<I: Input, F: FnMut((I::Token, u8)) -> u8>(mut f: F, i: I) -> u8 {
    f((i.token(), 0))
}
fn optional<I: Input, F: FnMut((Option<I::Token>, u8)) -> u8>(mut f: F, i: I) -> u8 {
    f((Some(i.token()), 0))
}
fn annotated<I: Input, F: FnMut(I::Slice) -> usize>(mut f: F, i: I) -> usize {
    f(i.slice())
}
fn returned<I: Input, F: FnMut() -> I::Token>(mut f: F, _: I) -> I::Token {
    f()
}
fn main() {
    let bytes = vec![b'+', b'-'];
    let a = tupled(
        |(x, _)| match x {
            b'+' => 1,
            b'-' => 2,
            _ => 0,
        },
        Bytes(&bytes),
    );
    let b = optional(
        |(x, _)| match x {
            Some(b'+') => 1,
            Some(b'-') => 2,
            _ => 0,
        },
        Bytes(&bytes[1..]),
    );
    let n = annotated(|x: &[u8]| x.len(), Bytes(&bytes));
    let r = returned(|| -> u8 { 73 }, Bytes(&bytes));
    assert_eq!((a, b, n, r), (1, 2, 2, 73));
    println!("{} {} {} {}", a, b, n, r);
}
