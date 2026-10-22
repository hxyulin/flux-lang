//! Miscompile (debug and -O): a closure whose only use of a captured
//! place is a by-value method call on a `Copy` part of it
//! (`p.1[1].0.is_positive()`) captures the place by move instead of by
//! reference, so the captured array is dropped with the closure, before
//! the tuple's other fields.
//!
//! rustc prints `drop 1`, `drop 2`, `drop 3`, then `Some(7)`... in field
//! order for both functions (the tuple is dropped whole when `by_method`
//! returns). flux prints `drop 2`, `drop 3`, `drop 1` for `by_method` (the
//! array goes when `map` drops the closure), and rustc's order for
//! `by_field`, where the same place is read without a method call; with
//! and without -O.
//!
//! Found by difftest (seed 106385 of seeds 100000..108000, reduced and
//! then edited by hand: a `Drop` value's hash printed out of order).
//!
//! Diagnosis: closure capture analysis (flux-ty/flux-mir, not codegen):
//! `flux mir` shows `{closure}(move _1.1, ...)` for `by_method`. In
//! rustc's edition-2021 rules a by-value use of a `Copy` place is a read
//! (`ByValue` only for non-`Copy` moves), and the place is captured by
//! shared reference, truncated at the index (`p.1`); the method call's
//! autoref-free receiver seems to be taken for a move.

struct D(i32);

impl Drop for D {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn by_method(p: (D, [D; 2])) -> Option<i32> {
    Some(0).map(|_x: i32| if p.1[1].0.is_positive() { 7 } else { 8 })
}

fn by_field(p: (D, [D; 2])) -> Option<i32> {
    Some(0).map(|_x: i32| if p.1[1].0 > 0 { 7 } else { 8 })
}

fn main() {
    println!("{:?}", by_method((D(1), [D(2), D(3)])));
    println!("{:?}", by_field((D(4), [D(5), D(6)])));
}
