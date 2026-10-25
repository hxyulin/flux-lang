use std::ops::{Index, IndexMut};
use std::slice::SliceIndex;
struct View<T>(Vec<T>);
impl<T, I: SliceIndex<[T]>> Index<I> for View<T> {
    type Output = I::Output;
    fn index(&self, i: I) -> &Self::Output { &self.0.as_slice()[i] }
}
impl<T, I: SliceIndex<[T]>> IndexMut<I> for View<T> {
    fn index_mut(&mut self, i: I) -> &mut Self::Output { &mut self.0.as_mut_slice()[i] }
}
struct Both<I: Iterator>(I);
impl<I: Iterator> Iterator for Both<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<Self::Item> { self.0.next() }
}
impl<I: DoubleEndedIterator> DoubleEndedIterator for Both<I> {
    fn next_back(&mut self) -> Option<Self::Item> { self.0.next_back() }
}
trait Source<A> { type Out; fn get(&self) -> Self::Out; }
trait Base<A, B> { type Out; }
trait Child<A, B>: Base<B, A> { fn forwarded(&self) -> Self::Out; }
struct Wrapper<T>(T);
impl<T: Source<u16>> Base<u16, u8> for Wrapper<T> { type Out = Option<T::Out>; }
impl<T: Source<u16>> Child<u8, u16> for Wrapper<T> {
    fn forwarded(&self) -> Self::Out { Some(self.0.get()) }
}
struct Borrowed<'a>(&'a u32);
impl<'a> Source<u16> for Borrowed<'a> {
    type Out = &'a u32;
    fn get(&self) -> Self::Out { self.0 }
}
struct Drain<'a, T> { marker: std::marker::PhantomData<&'a mut T>, count: usize }
impl<'a, T> Drain<'a, T> { fn fill(&mut self) { self.count += 1; } }
struct Splice<'a, I: Iterator> where I::Item: 'a { drain: Drain<'a, I::Item>, replace: I }
impl<I: Iterator> Drop for Splice<'_, I> { fn drop(&mut self) { self.drain.fill(); } }
fn splice<'a, T, I: Iterator<Item = T>>(_: &'a mut T, replace: I) -> Splice<'a, I> {
    Splice { drain: Drain { marker: std::marker::PhantomData, count: 0 }, replace }
}
fn main() {
    let mut anchor = String::from("anchor");
    let mut replacement = splice(&mut anchor, [String::from("replace")].into_iter());
    assert_eq!(replacement.replace.next().unwrap(), "replace");
    replacement.drain.fill();
    assert_eq!(replacement.drain.count, 1);
    drop(replacement);
    assert_eq!(anchor, "anchor");
    let mut view = View(vec![2u32, 3, 5, 7]);
    view[1] = 11;
    view[2..][0] = 13;
    assert_eq!(&view[..], &[2, 11, 13, 7]);
    {
        let mut both = Both(view.0.iter_mut());
        *both.next_back().unwrap() += 10;
        *both.next().unwrap() += 20;
    }
    assert_eq!(&view[..], &[22, 11, 13, 17]);
    let mut strings = Both(vec![String::from("front"), String::from("back")].into_iter());
    let back = strings.next_back().unwrap();
    let front = strings.next().unwrap();
    let value = 29;
    let wrapper = Wrapper(Borrowed(&value));
    let borrowed = wrapper.forwarded().unwrap();
    assert_eq!(borrowed as *const u32, &value as *const u32);
    println!("{} {} {} {} {front} {back} {}", view[0], view[1], view[2], view[3], borrowed);
}
