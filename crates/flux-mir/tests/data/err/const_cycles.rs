// Constants that depend on themselves (E0391). rustc's message names its
// internal query ("cycle detected when simplifying constant for the type
// system `A`") and points at the whole item; flux reports each use on the
// cycle.

const A: i32 = B + 1;
const B: i32 = A;
const SELF: i32 = SELF + 1;

const fn looped() -> i32 {
    LOOPED
}

const LOOPED: i32 = looped();

fn main() {
    let _ = A + SELF + LOOPED;
}
