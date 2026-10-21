//! The first program flux will compile.

unsafe extern "C" {
    fn putchar(c: i32) -> i32;
    fn exit(code: i32) -> !;
}

fn fib(n: i64) -> i64 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}

fn print_digit(d: i64) {
    unsafe {
        putchar(48 + d as i32);
    }
}

fn main() {
    let mut i = 0;
    while i < 10 {
        print_digit(fib(i) % 10);
        i += 1;
    }
    unsafe {
        exit((fib(20) % 256) as i32);
    }
}
