//! MISSING FEATURE. `bool::then_some` (and `then`).
//!
//! rustc: compiles; exits with status 4.
//! flux: "the method `then_some` on `bool` is not supported yet".
//! Blocks round2/bigint.rs and round2/backtracking.rs (and is used in
//! brainfuck.rs and json.rs behind `try_fold`).

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let x = (3 > 2).then_some(4).unwrap_or(0);
    unsafe { exit(x) }
}
