//! Functions and closures as values of each other's kinds: function
//! items and pointers implement the `Fn`-family traits, so they may be
//! passed where closures are; closures that capture nothing coerce to
//! function pointers.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

fn half(x: i32) -> Option<i32> {
    if x % 2 == 0 { Some(x / 2) } else { None }
}

fn double(x: i32) -> i32 {
    x * 2
}

fn id<T>(x: T) -> T {
    x
}

fn apply<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(x)
}

fn apply_mut<F: FnMut(i32) -> i32>(mut f: F, x: i32) -> i32 {
    f(x) + f(x)
}

fn main() {
    pointers();
    show(1, Some(8).and_then(half).and_then(half).unwrap_or(-1));
    show(2, apply(double, 5));
    let p: fn(i32) -> i32 = double;
    show(3, apply(p, 6));
    show(4, apply_mut(p, 7));
    let total: i32 = (1..4).map(double).sum();
    show(5, total);
    show(6, apply(id, 9));
    show(7, (0..10).filter_map(half).count() as i32);
}

fn run(f: fn(i32, i32) -> i32, a: i32, b: i32) -> i32 {
    f(a, b)
}

fn pointers() {
    let inc: fn(i32) -> i32 = |x| x + 1;
    show(1, inc(41));
    show(2, run(|a, b| a * b, 6, 7));
    let ops: [fn(i32, i32) -> i32; 2] = [|a, b| a + b, |a, b| a - b];
    show(3, ops[0](5, 3) * 10 + ops[1](5, 3));
    let pair: fn() -> (i32, i64) = || (1, 2);
    let (x, y) = pair();
    show(4, x + y as i32);
}
