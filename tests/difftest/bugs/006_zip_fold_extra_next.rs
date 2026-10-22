//! Miscompile (debug and -O), in `core`'s semantics: folding a `Zip`
//! whose first iterator is longer pulls one more item from the first
//! iterator than rustc does, running its `map` closure an extra time.
//!
//! rustc prints
//!
//! ```text
//! saw 1 1
//! saw 1 2
//! saw 3 10
//! 2 0 10
//! ```
//!
//! flux prints `saw 1 3` after `saw 1 2`, `saw 2 3` (the second zip pairs
//! nothing, yet runs the closure once) and `saw 3 11`, with and without
//! `-O`; the results agree.
//!
//! Found by difftest (seeds 229 and 729 of the first campaign, whose
//! side-effecting closures print their observations).
//!
//! Diagnosis: rustc's `Zip` specializes `fold` (and so `sum`, `count`,
//! `for_each`, ...) for `TrustedLen` iterators (ranges, slice and array
//! iterators, and `Map`/`Rev`/... over them): it asks `size_hint` for the
//! shorter length and pulls exactly that many pairs, never calling the
//! first iterator's `next` for a pair the second cannot complete (and
//! its `next` is specialized the same way for `TrustedRandomAccess`
//! iterators). flux's `Zip::fold` is the generic loop over `next`, which
//! calls `a.next()` and then finds `b` exhausted. Matching rustc means
//! the same specializations in flux's `core` (`library/core/src/iter.rs`)
//! for the iterators rustc marks `TrustedLen`. Not a codegen bug.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn see(tag: i32, x: i32) -> i32 {
    unsafe { printf(c"saw %d %d\n".as_ptr(), tag, x) };
    x
}

fn main() {
    let a = [1, 2, 3];
    // Two pairs: rustc calls `see` for 1 and 2 only.
    let s = a.iter().map(|x| see(1, *x)).zip(0..2).fold(0, |acc, (x, y)| acc + x * y);
    // Nothing to pair with: rustc calls `see` not at all.
    let t = a.into_iter().rev().map(|x| see(2, x)).zip(0..0).fold(0, |acc, (x, _)| acc + x);
    // `sum` folds too.
    let u: i32 = (10..13).map(|x| see(3, x)).zip(0..1).map(|(x, _)| x).sum();
    unsafe { printf(c"%d %d %d\n".as_ptr(), s, t, u) };
}
