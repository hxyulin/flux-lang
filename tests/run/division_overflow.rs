//! `i32::MIN / -1` overflows; it panics even without overflow checks.

fn divide(a: i32, b: i32) -> i32 {
    a / b
}

fn main() {
    divide(-2147483648, -1);
}
