// Function pointer types, and functions named as values.

struct Op {
    f: fn(i32) -> i32,
    check: fn(&i32, u8) -> bool,
    log: unsafe fn(),
}

fn double(x: i32) -> i32 {
    x * 2
}

fn positive(x: &i32, _: u8) -> bool {
    *x > 0
}

unsafe fn nothing() {}

fn apply(f: fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}

fn main() {
    let op = Op {
        f: double,
        check: positive,
        log: nothing,
    };
    let g = double;
    let h: fn(i32) -> i32 = g;
    let a = apply(double, (op.f)(g(1)));
    let b = (op.check)(&a, 0);
}
