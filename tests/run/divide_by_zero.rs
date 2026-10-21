//! Division by zero panics even without overflow checks; with
//! `panic=abort` the program aborts.

fn divide(a: u32, b: u32) -> u32 {
    a / b
}

fn main() {
    divide(1, 0);
}
