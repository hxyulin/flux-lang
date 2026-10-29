//! `impl<T> Pair<T, T>` and `impl Pair<u8, i32>` cannot apply to the same
//! type, so each may define `second`. rustc accepts this and prints `1 2`.
//! flux reports "duplicate definitions with name `second`": its
//! duplicate-definition check does not match the repeated `T`.
//!
//! Move this file to `tests/run` once flux compiles it.

struct Pair<A, B>(A, B);

impl<T> Pair<T, T> {
    fn second(&self) -> u8 {
        1
    }
}

impl Pair<u8, i32> {
    fn second(&self) -> u8 {
        2
    }
}

fn main() {
    let same = Pair(3u8, 4u8);
    let mixed = Pair(3u8, 4i32);
    println!("{} {}", same.second(), mixed.second());
}
