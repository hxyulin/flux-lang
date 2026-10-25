use core::slice::SliceIndex;

fn get<T, I: SliceIndex<[T]>>(slice: &[T], index: I) -> &I::Output { &slice[index] }
fn get_mut<T, I: SliceIndex<[T]>>(slice: &mut [T], index: I) -> &mut I::Output { &mut slice[index] }

trait Array { type Item; }
impl<T> Array for [T; 4] { type Item = T; }
trait Read<T> { fn read() -> u64; }
impl<A: Array, B> Read<(A::Item, B)> for (A, B) { fn read() -> u64 { 71 } }
fn projected<T>() -> u64 { <([T; 4], u64) as Read<(T, u64)>>::read() }

trait Change { fn change(self); }
impl Change for &mut u64 { fn change(self) { *self += 7; } }

fn main() {
    let mut values = [3u64, 5, 7, 11, 13];
    *get_mut(&mut values, 2usize) = 17;
    println!("index {} {:?} {:?}", get(&values, 2usize), get(&values, 1..4), get(&values, ..=2));
    let mut iterator = values.iter();
    assert_eq!(iterator.next(), Some(&3));
    assert_eq!(iterator.next_back(), Some(&13));
    println!("remaining {:?}", iterator.as_slice());
    let mut number = 1;
    let pointer = &mut number;
    pointer.change();
    pointer.change();
    println!("receiver {number} projection {}", projected::<u32>());
    let _: Option<alloc_error::Legacy> = None;
    println!("legacy {} {}", core::usize::MAX == usize::MAX, core::isize::MIN == isize::MIN);
}

mod alloc_error {
    extern crate alloc;
    #[allow(deprecated)]
    pub type Legacy = alloc::alloc::LayoutErr;
}
