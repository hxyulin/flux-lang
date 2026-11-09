use core::num::Wrapping;
use std::collections::{BinaryHeap, binary_heap::PeekMut};
use std::cell::Cell;
use std::rc::Rc;

fn wrapping() {
    let mut state = Wrapping(0x123456789abcdef0u64);
    state ^= state >> 12;
    state ^= state << 25;
    state ^= state >> 27;
    println!("{}", state);
    let mut small = Wrapping(255u8);
    small += Wrapping(2);
    small *= Wrapping(7);
    small -= Wrapping(9);
    small /= Wrapping(2);
    small %= Wrapping(13);
    small |= Wrapping(32);
    small &= Wrapping(63);
    small ^= Wrapping(17);
    small <<= 9usize;
    small >>= 10usize;
    println!("{}", small);
    assert_eq!((Wrapping(-3i128) >> 129usize).0, -2);
    assert_eq!((Wrapping(1u128) << 130usize).0, 4);
    assert_eq!((Wrapping(-128i8) / Wrapping(-1i8)).0, -128);
    assert_eq!((Wrapping(3u8) << usize::MAX).0, 128);
}

#[derive(Debug)]
struct Counted { key: u32, drops: Rc<Cell<u32>> }
impl PartialEq for Counted { fn eq(&self, rhs: &Self) -> bool { self.key == rhs.key } }
impl Eq for Counted {}
impl PartialOrd for Counted { fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> { Some(self.cmp(rhs)) } }
impl Ord for Counted { fn cmp(&self, rhs: &Self) -> core::cmp::Ordering { self.key.cmp(&rhs.key) } }
impl Drop for Counted { fn drop(&mut self) { self.drops.set(self.drops.get() + 1); } }

fn heap() {
    let mut empty = BinaryHeap::<u32>::new();
    assert!(empty.peek_mut().is_none());
    let mut heap = BinaryHeap::from([9, 5, 7, 1]);
    {
        let top = heap.peek_mut().unwrap();
        assert_eq!(*top, 9);
        println!("{:?}", top);
        core::mem::forget(top);
    }
    assert_eq!(heap.len(), 4);
    *heap.peek_mut().unwrap() = 2;
    assert_eq!(heap.peek(), Some(&7));
    {
        let mut top = heap.peek_mut().unwrap();
        *top = 10;
        assert_eq!(PeekMut::pop(top), 10);
    }
    assert_eq!(heap.clone().into_sorted_vec(), vec![1, 2, 5]);
    {
        let mut top = heap.peek_mut().unwrap();
        *top = 0;
    }
    assert_eq!(heap.into_sorted_vec(), vec![0, 1, 2]);
    let mut single = BinaryHeap::from([7]);
    *single.peek_mut().unwrap() = 3;
    assert_eq!(PeekMut::pop(single.peek_mut().unwrap()), 3);
    assert!(single.is_empty());
    let drops = Rc::new(Cell::new(0));
    {
        let mut heap = BinaryHeap::new();
        for key in [1, 2, 3] { heap.push(Counted { key, drops: drops.clone() }); }
        let mut top = heap.peek_mut().unwrap();
        top.key = 0;
        core::mem::forget(top);
        assert_eq!(heap.len(), 1);
    }
    assert_eq!(drops.get(), 1);
    {
        let mut heap = BinaryHeap::new();
        for key in [4, 5, 6] { heap.push(Counted { key, drops: drops.clone() }); }
        heap.peek_mut().unwrap().key = 0;
        assert_eq!(heap.peek().unwrap().key, 5);
        drop(PeekMut::pop(heap.peek_mut().unwrap()));
    }
    assert_eq!(drops.get(), 4);
    println!("drops {}", drops.get());
}
struct Always<T, F: Fn() -> T>(F);
impl<T, F: Fn() -> T> Always<T, F> { fn get(&self) -> T { (self.0)() } }
trait Once<A> { type Output; fn call(self, arg: A) -> Self::Output; }
struct OkFn<E>(core::marker::PhantomData<fn(E)>);
impl<E> Copy for OkFn<E> {}
impl<E> Clone for OkFn<E> { fn clone(&self) -> Self { *self } }
impl<A, E> Once<A> for OkFn<E> { type Output = Result<A,E>; fn call(self, arg:A) -> Self::Output { Ok(arg) } }
trait Mut<A>: Once<A> { fn call_mut(&mut self, arg:A) -> Self::Output; }
impl<E,A> Mut<A> for OkFn<E> where Self: Once<A> { fn call_mut(&mut self,arg:A) -> Self::Output { self.call(arg) } }
fn main() {
    wrapping(); heap();
    println!("{}", Always(|| 17u32).get());
    let mut f = OkFn::<u8>(core::marker::PhantomData);
    println!("{:?}", f.call_mut(23u32));
}
