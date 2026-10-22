//! Infer fold accumulator types before checking unannotated closures,
//! including when the call is an operand of an arithmetic expression.

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
