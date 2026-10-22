// Constants in patterns: errors found during lowering.

const N: i32 = 5;

fn f() {}

struct S;

impl S {
    fn g() {}
}

fn shadowing(x: i32) {
    match x {
        ref N => {}
    }
    match x {
        mut N => {}
    }
    let ref N = x;
}

fn shadowing_param(N @ 1: i32) {}

fn not_constants(x: i32) -> i32 {
    match x {
        crate::f => 1,
        S::g => 2,
        _ => 0,
    }
}

fn range_ends(x: i32) -> i32 {
    let y = 3;
    match x {
        0..=y => 1,
        0..=f => 2,
        0..=Z => 3,
        _ => 0,
    }
}

fn main() {}
