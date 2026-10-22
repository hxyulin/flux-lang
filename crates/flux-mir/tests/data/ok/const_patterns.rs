// Constants in patterns compare with the constant's value: alone, and
// as the ends of ranges.

const N: i32 = 5;
const ON: bool = true;

struct S;

impl S {
    const HIGH: i32 = 10;
}

fn classify(x: i32) -> i32 {
    match x {
        N => 0,
        i32::MIN..N => 1,
        6..=S::HIGH => 2,
        _ => 3,
    }
}

fn flag(b: bool) -> i32 {
    match b {
        ON => 1,
        false => 0,
    }
}

fn main() {}
