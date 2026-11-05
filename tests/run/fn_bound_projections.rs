//! Fn bounds whose arguments name `Self::Item`, normalized through the impl.
trait PeekingNext: Iterator {
    fn peeking_next<F>(&mut self, accept: F) -> Option<Self::Item> where Self: Sized, F: FnOnce(&Self::Item) -> bool;
}
struct W<I>(I, Option<u8>);
impl<I: Iterator> Iterator for W<I> { type Item = I::Item; fn next(&mut self) -> Option<I::Item> { self.0.next() } }
impl<I: Iterator> PeekingNext for W<I> {
    fn peeking_next<F>(&mut self, accept: F) -> Option<Self::Item> where F: FnOnce(&Self::Item) -> bool {
        let r: I::Item = self.0.next()?;
        if !accept(&r) { return None; }
        Some(r)
    }
}
macro_rules! by_clone {
    ([$($typarm:tt)*] $type_:ty) => {
        impl<$($typarm)*> PeekingNext for $type_ {
            fn peeking_next<F>(&mut self, accept: F) -> Option<Self::Item>
                where F: FnOnce(&Self::Item) -> bool
            {
                let saved_state = self.clone();
                if let Some(r) = self.next() {
                    if !accept(&r) { *self = saved_state; } else { return Some(r) }
                }
                None
            }
        }
    }
}
by_clone! { [I: Clone + DoubleEndedIterator] ::std::iter::Rev<I> }
by_clone! { ['a, T] ::std::slice::Iter<'a, T> }
fn main() {
    let mut w = W(vec![1u32, 5].into_iter(), None);
    println!("{:?}", w.peeking_next(|x| *x > 2));
    let mut r = vec![1u32, 5].into_iter().rev();
    println!("{:?}", r.peeking_next(|x| *x > 2));
    let v = [1u32, 5];
    let mut s = v.iter();
    println!("{:?}", s.peeking_next(|x| **x > 2));
}
