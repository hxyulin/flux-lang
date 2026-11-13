use std::ops::Add;

#[derive(Debug, PartialEq)]
struct Span { start: usize, end: usize }
impl Add<usize> for Span {
    type Output = Self;
    fn add(self, offset: usize) -> Self::Output {
        Self::Output { start: self.start + offset, end: self.end + offset }
    }
}
impl Add<Span> for usize {
    type Output = Span;
    fn add(self, span: Span) -> Self::Output {
        Self::Output { start: self + span.start, end: self + span.end }
    }
}
struct Pair<T> { left: T, right: u8 }
trait Build<T> { type Output; fn build(value: T) -> Self::Output; }
impl<T> Build<T> for usize {
    type Output = Pair<T>;
    fn build(value: T) -> Self::Output {
        let Self::Output { left, right } = Self::Output { left: value, right: 9 };
        Self::Output { left, right }
    }
}
fn main() {
    let span = 7usize + (Span { start: 1, end: 3 } + 2usize);
    assert_eq!(span, Span { start: 10, end: 12 });
    let pair = <usize as Build<u64>>::build(42);
    assert_eq!(pair.left, 42u64);
    assert_eq!(pair.right, 9);
    println!("{span:?} {} {}", pair.left, pair.right);
}
