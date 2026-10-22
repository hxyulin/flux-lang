//! Wrong rejection: a closure passed to `fold` cannot call a method on
//! its accumulator when the `fold` call is an operand of `+`.
//!
//! rustc accepts this and exits with 26. flux reports "the method
//! `wrapping_mul` on `_` is not supported yet" in `in_sum` only (debug
//! and -O alike): the same `fold` alone is fine, and so is the closure
//! with its parameter's type written (`|acc: u16, _x|`).
//!
//! Found by difftest (seed 274 of the first campaign, reduced from 1217
//! bytes to the `in_sum` line; the methods vary: `checked_add`,
//! `wrapping_neg`, `div_ceil`, `clamp`, ...). `--weight
//! unannotated_closures=0` writes closure parameters' types.
//!
//! Diagnosis: as an operand of a binary operator, the `fold` call is
//! checked with an expectation that delays unifying `B` with the type of
//! the initial value, so the closure body is checked while `acc` is still
//! an inference variable, and a method call on an inference variable is
//! an error instead of being deferred until `B = u16` is known (rustc
//! checks closure arguments after the other arguments).

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn alone() -> u16 {
    (0i32..3).fold(1u16, |acc, _x| acc.wrapping_mul(2))
}

fn in_sum() -> u16 {
    (0i32..3).fold(1u16, |acc, _x| acc.wrapping_mul(2)) + 1u16
}

fn annotated() -> u16 {
    (0i32..3).fold(1u16, |acc: u16, _x| acc.wrapping_mul(2)) + 1u16
}

fn main() {
    unsafe { exit((alone() + in_sum() + annotated()) as i32) }
}
