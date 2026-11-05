//! Forty nested `Map` adapters, more than the 32 nested goals at which
//! `select_impl` stops. rustc accepts this and prints `123`. flux
//! type-checks it, but its borrow checker's region proofs take about
//! 29 s in a release build: each proof selects the adapters' impls
//! again, at a cost that grows with the fourth power of their number.
//! (They took time exponential in it, 61 s for 14 adapters, before
//! `select_impl` remembered its nested answers.)
//!
//! Move this file to `tests/run` once flux compiles it.

fn total(n: u32) -> u32 {
    (0..n)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .map(|x| x + 1)
        .sum()
}

fn main() {
    println!("{}", total(3));
}
