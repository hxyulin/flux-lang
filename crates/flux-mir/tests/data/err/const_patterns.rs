// Constants in patterns: checks that need their values, which constant
// evaluation computes first.

const N: i32 = 5;
const LOW: u8 = 10;
const HIGH: u8 = 2;
const YES: bool = true;
const BAD: i32 = i32::MAX + 1;

const fn double(x: u8) -> u8 {
    x * 2
}

const TWENTY: u8 = double(LOW);

struct S;

impl S {
    const LIMIT: u8 = 200;
}

fn missing_zero(x: i32) -> i32 {
    match x {
        i32::MIN..=-1 => 0,
        1..=i32::MAX => 1,
    }
}

fn missing_false(x: bool) -> i32 {
    match x {
        YES => 1,
    }
}

fn empty_range(x: u8) -> i32 {
    match x {
        LOW..=HIGH => 0,
        _ => 1,
    }
}

fn computed(x: u8) -> i32 {
    match x {
        0..TWENTY => 0,
        TWENTY..=S::LIMIT => 1,
    }
}

fn refutable_let(x: i32) {
    let N = x;
}

fn refutable_param(N: i32) {}

fn failing(x: i32) -> i32 {
    match x {
        BAD => 0,
        _ => 1,
    }
}

// rustc does not borrow-check a body whose patterns have errors.
fn not_borrow_checked(x: u8) -> i32 {
    let v: i32;
    match x {
        LOW => v,
    }
}

fn main() {}
