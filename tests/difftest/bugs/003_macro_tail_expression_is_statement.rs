//! Wrong rejection: a macro call ending a block, whose expansion is a
//! block-like expression (`matches!` expands to a `match`), is taken for
//! a statement, so the block's value is `()`.
//!
//! rustc accepts this and exits with 7. flux reports, for `tail` only,
//! "mismatched types: expected `bool`, found `()`" at the return type and
//! "expected `()`, found `bool`" at the call (debug and -O alike); the
//! same call bound with `let` or parenthesized is fine.
//!
//! Found by difftest (a matches! at the end of a closure's or an `if`
//! branch's block; `--weight bare_macros=0` parenthesizes them).
//!
//! Diagnosis: in rustc, a macro call in statement position whose
//! expansion is an expression that ends the block is the block's tail
//! expression; flux seems to apply the "a block-like expression ends a
//! statement" rule to the expanded `match` and treat it as an expression
//! statement.

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn let_bound(x: i64) -> bool {
    let r = matches!(x, 1 | 3);
    r
}

fn paren_tail(x: i64) -> bool {
    (matches!(x, 1 | 3))
}

fn tail(x: i64) -> bool {
    matches!(x, 1 | 3)
}

fn main() {
    let x = core::hint::black_box(3);
    unsafe { exit(let_bound(x) as i32 + paren_tail(x) as i32 * 2 + tail(x) as i32 * 4) }
}
