//! MISSING FEATURE (documented). Closures passed to generic functions that
//! return references into their parameters: `filter_map(|s| match s {
//! Slot::Full(v) => Some(v), .. })`, `map(|(t, _)| t)` on a tuple holding a
//! reference, `map(|(slot, f)| *slot = f)`.
//!
//! rustc: compiles; exits with status 12.
//! flux: "closures that keep references to their parameters are not
//! supported yet". Blocks round2/hashmap.rs, round2/json.rs and
//! round2/ini_csv.rs; in hashmap.rs it is followed by seven spurious
//! "cannot borrow `counts` as mutable because it is also borrowed as
//! immutable" errors that disappear once the closure is rewritten.

enum Slot {
    Empty,
    Full(i32),
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let slots = [Slot::Full(5), Slot::Empty, Slot::Full(7)];
    let total: i32 = slots
        .iter()
        .filter_map(|s| match s {
            Slot::Full(v) => Some(v),
            Slot::Empty => None,
        })
        .sum();
    unsafe { exit(total) }
}
