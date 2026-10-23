//! Dropless slices must finish even at usize::MAX length. Drop-bearing ZSTs
//! still run their destructors, as do owners and fields of generic aggregates.
use std::cell::Cell;
use std::mem::{ManuallyDrop, MaybeUninit, needs_drop, size_of};

#[repr(align(64))]
struct AlignedZst;

struct Nested<T>((T, [MaybeUninit<T>; 2]));

enum Choice<T> {
    Value(T),
    Empty,
}

// Only MaybeUninit storage is made uninitialized; no slot is ever read.
fn uninit<T>(len: usize) -> Vec<MaybeUninit<T>> {
    let mut v = Vec::with_capacity(len);
    // SAFETY: all bit patterns (including uninitialized) are valid wrappers.
    // with_capacity reserves len slots, including usize::MAX for a ZST.
    unsafe { v.set_len(len) };
    v
}

fn dropless<T>(len: usize) {
    assert!(!needs_drop::<MaybeUninit<T>>());
    drop(uninit::<T>(len));
    drop(uninit::<T>(len).into_boxed_slice());
    let mut v = uninit::<T>(len);
    // SAFETY: these are live MaybeUninit slots. Their drop does nothing, so
    // leaving the wrappers in v to be dropped again is harmless.
    unsafe { std::ptr::drop_in_place(v.as_mut_slice()) };
    drop(v);
}

fn initialized_zsts(len: usize) {
    let mut v = Vec::<Nested<AlignedZst>>::with_capacity(len);
    // SAFETY: Nested<AlignedZst> has size zero and is inhabited; its fields
    // are the unit-like AlignedZst and MaybeUninit wrappers. No bytes need
    // initializing, and Vec reserves usize::MAX slots for these ZSTs.
    unsafe { v.set_len(len) };
    assert!(!needs_drop::<Nested<AlignedZst>>());
    // SAFETY: all elements are initialized and have no destructor. Dropping
    // them in place leaves them valid for the subsequent owner drop.
    unsafe { std::ptr::drop_in_place(v.as_mut_slice()) };
    drop(v.into_boxed_slice());
}

struct Owner<'a, T> {
    drops: &'a Cell<usize>,
    values: Box<[T]>,
}

impl<T> Drop for Owner<'_, T> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        println!("owner {}", self.values.len());
    }
}

#[repr(align(64))]
struct Noisy;
impl Drop for Noisy {
    fn drop(&mut self) {
        println!("zst");
    }
}

struct Tail;
impl Drop for Tail {
    fn drop(&mut self) {
        println!("tail");
    }
}

fn generic<T>(values: Vec<T>, drops: &Cell<usize>) {
    drop(Owner {
        drops,
        values: values.into_boxed_slice(),
    });
}

fn main() {
    assert_eq!(size_of::<AlignedZst>(), 0);
    assert_eq!(size_of::<Noisy>(), 0);
    assert!(needs_drop::<Noisy>());
    assert!(!needs_drop::<ManuallyDrop<Noisy>>());
    for len in [0, 1, 17, usize::MAX] {
        dropless::<AlignedZst>(len);
        dropless::<Nested<AlignedZst>>(len);
        dropless::<Noisy>(len); // MaybeUninit suppresses even a ZST destructor.
        initialized_zsts(len);
    }
    for len in [0, 1, 17] {
        dropless::<u64>(len);
        dropless::<[u64; 4]>(len);
        dropless::<Choice<u64>>(len);
    }
    // Actual initialized aggregates, arrays, and slices, rather than just
    // MaybeUninit: generic substitutions must retain the element's drop glue.
    let drops = Cell::new(0);
    generic(uninit::<Noisy>(usize::MAX), &drops);
    generic(vec![AlignedZst, AlignedZst], &drops);
    generic(vec![Noisy, Noisy], &drops);
    generic(Vec::<Noisy>::new(), &drops);
    generic(vec![(Noisy, Tail), (Noisy, Tail)], &drops);
    generic(vec![Choice::Value(Noisy), Choice::Empty], &drops);
    generic(
        vec![Nested((Noisy, [const { MaybeUninit::uninit() }; 2]))],
        &drops,
    );
    generic(vec![ManuallyDrop::new(Noisy)], &drops);
    drop(Box::new([Noisy, Noisy]));
    let empty: [Noisy; 0] = [];
    drop(Box::new(empty));
    let mut array = ManuallyDrop::new([Noisy, Noisy]);
    // SAFETY: drop the initialized elements once; ManuallyDrop prevents reuse.
    unsafe { std::ptr::drop_in_place(&mut array[..]) };
    drop(Box::new([AlignedZst, AlignedZst]));
    drop(Box::new([Nested((
        AlignedZst,
        [const { MaybeUninit::uninit() }; 2],
    ))]));
    // These nonzero allocations must still be freed through the box owner.
    generic(vec![String::from("one"), String::from("two")], &drops);
    generic(vec![11_u64, 22_u64], &drops);
    assert_eq!(drops.get(), 10);
    println!("dropped");
}
