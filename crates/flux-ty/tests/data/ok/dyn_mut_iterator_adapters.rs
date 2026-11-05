// By-value adapters on `&mut dyn Iterator` select `Self = &mut dyn
// Iterator` through `impl Iterator for &mut I`, never the unsized
// `dyn Iterator`.

use std::iter::Peekable;

struct Held<I: Iterator> {
    iter: I,
    cached: Option<I::Item>,
}

fn hold<I: Iterator>(iter: I) -> Held<I> {
    Held { iter, cached: None }
}

fn generic<I: ?Sized + Iterator<Item = i32>>(iter: &mut I) -> Peekable<&mut I> {
    iter.peekable()
}

fn adapters(r: &mut dyn Iterator<Item = i32>) -> Option<i32> {
    let mut p = r.peekable();
    let first = p.peek().copied();
    let mapped = r.map(|x| x + 1).next();
    let taken = r.take(1).next();
    let mut held = hold(r);
    held.cached = held.iter.next();
    first.or(mapped).or(taken).or(held.cached)
}

fn boxed(mut b: Box<dyn Iterator<Item = i32>>) -> Option<i32> {
    let first = b.by_ref().peekable().next();
    let mut p = b.peekable();
    first.or(p.next())
}

fn main() {
    let mut it = vec![1, 2, 3].into_iter();
    adapters(&mut it);
    generic(&mut it).peek();
    boxed(Box::new(it));
}
