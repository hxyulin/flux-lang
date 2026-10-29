//! Forty nested `Map` adapters, more than the 32 nested goals at which
//! `select_impl` stops. rustc accepts this and prints `123`. flux
//! type-checks it, but its borrow checker's region proofs take time
//! exponential in the number of adapters (about 1 s for 10 and 61 s for
//! 14 in a release build), so it does not finish.
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
