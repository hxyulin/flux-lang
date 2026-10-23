//! Manual constant-memory probe; no generator seed.
//! Valid Rust: rustc prints 1 in both optimization modes. Flux copies the
//! structural Cell aggregate through a reinterpreted pointer without applying
//! the target memory layout, then serializes it as if it were the stored ref.
//! Observed outputs were 0 without optimization and 8 with optimization;
//! those incorrect values are incidental. Reproduced with both the candidate
//! const-borrow fix and the preserved pre-type-name compiler (15e9cb3).
use std::cell::Cell;

const X: &u8 = unsafe {
    // SAFETY: Cell<T> has T's memory layout, the temporary remains alive for
    // this copy, and the returned reference points to the promoted immutable 1.
    *(&Cell::new(&1u8) as *const Cell<&u8> as *const &u8)
};

fn main() {
    println!("{}", X);
}
