//! Loops, `break` with values, `continue`, early returns, short-circuit
//! evaluation and recursion.

unsafe extern "C" {
    fn putchar(c: i32) -> i32;
    fn exit(code: i32) -> !;
}

fn out(c: i32) {
    unsafe {
        putchar(c);
    }
}

/// Prints `c` and returns `value`, to show which operands are evaluated.
fn trace(c: i32, value: bool) -> bool {
    out(c);
    value
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn first_square_above(n: u32) -> u32 {
    let mut i = 0;
    loop {
        if i * i > n {
            break i;
        }
        i += 1;
    }
}

fn sum_odd_below(n: i32) -> i32 {
    let mut sum = 0;
    let mut i = 0;
    while i < n {
        i += 1;
        if i % 2 == 0 {
            continue;
        }
        sum += i;
    }
    sum
}

fn classify(n: i32) -> i32 {
    if n < 0 {
        return 45;
    }
    if n == 0 { 48 } else if n < 10 { 115 } else { 108 }
}

fn main() {
    if trace(97, false) && trace(98, true) {
        out(33);
    }
    if trace(99, true) || trace(100, true) {
        out(101);
    }
    let both = trace(102, true) && trace(103, false);
    out(if both { 49 } else { 48 });
    out(10);
    out(classify(-5));
    out(classify(0));
    out(classify(7));
    out(classify(70));
    out(10);
    let x;
    if gcd(1071, 462) == 21 {
        x = 1;
    } else {
        x = 2;
    }
    unsafe {
        exit(x * 1000 % 256 + (first_square_above(50) + sum_odd_below(10) as u32) as i32);
    }
}
