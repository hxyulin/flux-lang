//! Wrong rejection: `StepBy` and `Zip` are not `DoubleEndedIterator`s in
//! flux's `core`, so `.rev()` after them is rejected.
//!
//! rustc accepts this and exits with 79 (rustc's `StepBy<I>` is
//! double-ended where `I: DoubleEndedIterator + ExactSizeIterator`, and
//! `Zip<A, B>` where both are). flux reports "the trait bound
//! `StepBy<Iter<u32>>: DoubleEndedIterator` is not satisfied" and the same
//! for `Zip<Iter<u32>, Iter<u32>>` (debug and -O alike). LANGUAGE.md says
//! the adapters implement `DoubleEndedIterator` where rustc's do.
//! (Chain's impl works; `Skip`, `Take` and `Enumerate` over exact-size
//! iterators were also seen rejected after `rev` in campaigns, in
//! combinations with these two.)
//!
//! Found by difftest (`--weight rev_adapters=0` keeps `rev` after
//! adapters out of programs).
//!
//! Diagnosis: missing impls in `library/core/src/iter.rs`.

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let a = [1u32, 2, 3, 4, 5];
    let b = [10u32, 20, 30];
    let s: u32 = a.iter().step_by(2).rev().sum();
    let z: u32 = a.iter().zip(b.iter()).rev().map(|(x, y)| x * y).sum();
    let c: u32 = (0u32..3).chain(5u32..7).rev().fold(0, |acc, x| acc * 10 + x);
    unsafe { exit((s + z + c) as i32 % 256) }
}
