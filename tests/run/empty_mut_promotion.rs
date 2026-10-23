use std::cell::Cell;

struct NoDefault;
enum Never {}
struct Dropping;
impl Drop for Dropping {
    fn drop(&mut self) {
        panic!("empty array must not drop an element");
    }
}
#[repr(align(64))]
struct Aligned(u8);
#[repr(align(64))]
struct AlignedZst;

const fn empty<'a, T>() -> &'a mut [T] {
    &mut []
}
fn array<T>() -> &'static mut [T; 0] {
    &mut []
}
struct Storage<T: 'static> {
    remaining: &'static mut [T],
}
impl<T: 'static> Storage<T> {
    const fn new() -> Self {
        Self { remaining: &mut [] }
    }
}
const INITIAL: Storage<Aligned> = Storage::new();

fn check<T: 'static>() {
    let a = empty::<T>();
    let b = empty::<T>();
    assert_eq!(a.len(), 0);
    assert_eq!(b.len(), 0);
    assert_eq!(a.as_ptr().addr() % std::mem::align_of::<T>(), 0);
    assert_eq!(b.as_ptr().addr() % std::mem::align_of::<T>(), 0);
    // Moving either exclusive reference does not invalidate the other.
    let moved = a;
    assert!(b.is_empty());
    assert!(moved.is_empty());
    assert!(array::<T>().is_empty());
}
fn from_borrow<'a>(_: &'a u8) -> &'a mut [&'a u8] {
    &mut []
}
fn main() {
    check::<u64>();
    check::<NoDefault>();
    check::<Never>();
    check::<Dropping>();
    check::<Cell<u8>>();
    check::<Aligned>();
    check::<AlignedZst>();
    check::<&u8>();
    let initial = INITIAL;
    assert!(initial.remaining.is_empty());
    assert_eq!(initial.remaining.as_ptr().addr() % 64, 0);
    let local = 7;
    assert!(from_borrow(&local).is_empty());
    let aligned = Aligned(9);
    println!("empty-promotions {} {}", aligned.0, initial.remaining.len());
}
