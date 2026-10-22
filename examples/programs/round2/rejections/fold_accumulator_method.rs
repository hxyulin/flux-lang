//! WRONG REJECTION. A method call on `fold`'s accumulator inside the
//! closure, where the accumulator's type comes from the `init` argument.
//!
//! rustc: compiles; exits with status 6.
//! flux: "error: the method `wrapping_add` on `_` is not supported yet" at
//! `acc.wrapping_add`. rustc checks `init` first (fixing `B = i32`), then
//! deduces the closure's signature from `F: FnMut(B, Self::Item) -> B`;
//! flux seems to check the closure body before `B` is known. Operators on
//! the accumulator (`acc + a`) work, as they are resolved later. Found in
//! round2/fixed_vec.rs (`fold(0i32, |acc, (a, b)| acc.wrapping_mul(31)...)`).

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let x = [1, 2, 3];
    let r = x.iter().fold(0i32, |acc, &a| acc.wrapping_add(a));
    unsafe { exit(r) }
}
