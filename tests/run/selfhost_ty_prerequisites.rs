use std::cell::Cell;
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicUsize, Ordering};

std::thread_local! {
    static VALUE: Cell<u32> = Cell::new(9);
    static BYPASS: Cell<u32> = panic!("set must skip the initializer");
}
#[derive(PartialEq, Eq)]
struct Kind(u32);
const ERROR: Kind = Kind(0);
#[derive(PartialEq, Eq)]
struct Nested { flag: bool, bytes: [u8; 2] }
const NESTED: Nested = Nested { flag: true, bytes: [3, 5] };
fn classify(value: Kind) -> u32 { match value { ERROR => 10, _ => 20 } }
fn read(value: &u64) -> u64 { *value }
fn read_two(a: &u64, b: &u64) -> u64 { *a + *b }
fn mutate(a: &mut u64) -> u64 { *a += 1; *a }
struct Owner<'a>(&'a Cell<usize>, usize);
impl Drop for Owner<'_> { fn drop(&mut self) { self.0.set(self.0.get() + 1); } }
fn main() {
    assert_eq!(classify(Kind(0)), 10); assert_eq!(classify(Kind(1)), 20);
    assert_eq!(match (Nested { flag: true, bytes: [3, 6] }) { NESTED => 1, _ => 2 }, 2);
    let callable: &dyn Fn(&u64) -> u64 = &read;
    assert_eq!(callable(&7), 7);
    let both: &dyn Fn(&u64, &u64) -> u64 = &read_two;
    assert_eq!(both(&11, &13), 24);
    let mutable: &dyn Fn(&mut u64) -> u64 = &mutate;
    let mut n = 17; assert_eq!(mutable(&mut n), 18);
    let pointer: fn(&u64) -> u64 = read;
    let callable: &dyn Fn(&u64) -> u64 = &pointer;
    assert_eq!(callable(&19), 19);
    let slice = Some(&[1u32, 2, 3][..]);
    assert_eq!(slice.map_or(0, <[u32]>::len), 3);
    let a = [1u32, 2]; let b = [3u32];
    assert_eq!([&a[..], &b[..]].concat(), [1, 2, 3]);
    let mut v = vec![0, 1, 2, 3, 4];
    { let mut replaced = v.splice(1..4, [7, 8]); assert_eq!(replaced.next(), Some(1)); assert_eq!(replaced.next_back(), Some(3)); assert_eq!(replaced.len(), 1); }
    assert_eq!(v, [0, 7, 8, 4]);
    drop(v.splice(1..1, [9, 10, 11]));
    assert_eq!(v, [0, 9, 10, 11, 7, 8, 4]);
    drop(v.splice(1.., [])); assert_eq!(v, [0]);
    BYPASS.set(42); assert_eq!(BYPASS.get(), 42);
    assert_eq!(VALUE.get(), 9); VALUE.set(13);
    let count = Arc::new(AtomicUsize::new(0));
    let winner = OnceLock::new();
    std::thread::scope(|scope| {
        for index in 0..8 {
            let count = count.clone(); let winner = &winner;
            scope.spawn(move || {
                assert_eq!(VALUE.get(), 9);
                VALUE.set(index as u32);
                assert_eq!(VALUE.get(), index as u32);
                if winner.set(index).is_ok() { count.fetch_add(1, Ordering::SeqCst); }
            });
        }
    });
    assert_eq!(VALUE.get(), 13);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(*winner.get().unwrap() < 8);
    assert_eq!(winner.set(99), Err(99));
    let drops = Cell::new(0);
    {
        let mut values = vec![Owner(&drops, 1), Owner(&drops, 2), Owner(&drops, 3), Owner(&drops, 4)];
        let mut replacement = values.splice(1..3, [Owner(&drops, 5)]);
        assert_eq!(replacement.next().unwrap().1, 2);
        assert_eq!(drops.get(), 1);
        drop(replacement);
        assert_eq!(drops.get(), 2);
        assert_eq!(values.iter().map(|value| value.1).sum::<usize>(), 10);
    }
    assert_eq!(drops.get(), 5);
    {
        let once = OnceLock::new();
        assert!(once.set(Owner(&drops, 6)).is_ok());
        drop(once.set(Owner(&drops, 7)).err().unwrap());
        assert_eq!(drops.get(), 6);
    }
    assert_eq!(drops.get(), 7);
    let mut queue = std::collections::VecDeque::new();
    for index in 0..200 {
        queue.push_back(index);
        if index % 3 == 0 { assert_eq!(queue.pop_front(), Some(index / 3)); }
    }
    queue.push_front(99); assert_eq!(queue.pop_front(), Some(99));
    assert_eq!(queue.front(), Some(&67)); assert_eq!(queue.back(), Some(&199));
    assert_eq!(queue.len(), 133);
    let mut cloned = queue.clone();
    assert_eq!(format!("{queue:?}"), format!("{cloned:?}"));
    for expected in 67..200 { assert_eq!(queue.pop_front(), Some(expected)); }
    for expected in (67..200).rev() { assert_eq!(cloned.pop_back(), Some(expected)); }
    assert!(queue.is_empty()); assert!(cloned.is_empty());
    queue.push_back(1); queue.clear(); assert_eq!(queue.pop_front(), None);
    println!("ty prerequisites {} {} {}", count.load(Ordering::SeqCst), VALUE.get(), drops.get());
}
