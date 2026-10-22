//! MISSING FEATURE. `core::cmp::Reverse`, the usual way to turn a max-heap
//! into a min-heap or sort descending by key.
//!
//! rustc: compiles; exits with status 9.
//! flux: "`core::cmp::Reverse` is not supported yet", then "cannot find
//! struct, variant or union type `Reverse` in this scope" at each use.
//! Blocks round2/graphs.rs and round2/inventory.rs.

use core::cmp::Reverse;

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let mut xs = [3, 9, 1];
    xs.sort_by_key(|&x| Reverse(x));
    unsafe { exit(xs[0]) }
}
