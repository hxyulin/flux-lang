// Exercise short, spilled and nested type lists through generic substitution.
struct Wrapper<T> {
    value: T,
}

fn ends<T>(values: (T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T)) -> (T, T) {
    (values.0, values.19)
}

fn unwrap<T>(value: Wrapper<T>) -> T {
    value.value
}

fn apply(f: fn(i32) -> i32, value: i32) -> i32 {
    f(value)
}

fn increment(value: i32) -> i32 {
    value + 1
}

fn main() {
    let pair = ends((
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ));
    let nested = unwrap(Wrapper {
        value: (pair, [21, 22]),
    });
    println!("{} {} {}", nested.0.0, nested.0.1, nested.1[1]);
    let flags = ends((
        true, false, true, false, true, false, true, false, true, false, true, false, true, false,
        true, false, true, false, true, false,
    ));
    println!("{} {} {}", flags.0, flags.1, apply(increment, nested.1[0]));
}
