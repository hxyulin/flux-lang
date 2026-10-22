//! Output mismatch (debug and -O), in `core`: skipping elements from the
//! back of an array's `IntoIter` (`rev().skip(n)`, `nth_back`) drops
//! them back to front; rustc drops them front to back.
//!
//! rustc prints `drop 1`, `drop 2`, `drop 3` for the first statement and
//! `drop 5`, `drop 6`, `drop 4` for the second; flux prints `drop 3`,
//! `drop 2`, `drop 1` and `drop 6`, `drop 5`, `drop 4`, with and without
//! -O.
//! Skipping from the front (`skip`, `nth`) matches.
//!
//! Found by difftest (seed 106869 of seeds 100000..108000, reduced: a
//! `Drop` value's hash printed out of order, from `into_iter().rev()
//! .skip(3)` over a two-element array).
//!
//! Diagnosis: rustc's `array::IntoIter::advance_back_by` (which `nth_back`
//! and so `Rev::nth` use) drops the skipped tail as one slice
//! (`ptr::drop_in_place` of `[T]`, element by element from its start);
//! flux's drops them one at a time from the back. A fix in
//! `library/core/src/array.rs`'s `IntoIter`.

struct D(i32);

impl Drop for D {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    let n = [D(1), D(2), D(3)].into_iter().rev().skip(5).count();
    println!("rev skip: {n}");
    let mut it = [D(4), D(5), D(6)].into_iter();
    let x = it.nth_back(2).map(|d| d.0);
    println!("nth_back: {x:?}");
}
