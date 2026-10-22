//! Shared generic type nodes and interleaved captures preserve their values.
unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

type Pair<T> = (T, T);
type Quad<T> = (Pair<T>, Pair<T>);
type Octet<T> = (Quad<T>, Quad<T>);

fn identity<T>(value: Octet<T>) -> Octet<T> {
    value
}

struct Fields(i32, i32);

fn evaluate(offset: i32) -> i32 {
    let value = identity((((1, 2), (3, 4)), ((5, 6), (7, 8))));
    let mut a = Fields(value.0.1.0, value.0.1.1);
    let mut b = Fields(value.1.0.0, value.1.0.1);
    let mut capture = || {
        let first = b.1;
        let second = a.1;
        let third = b.0;
        a.0 = first + third + offset;
        b.1 = second + offset;
        let whole = &a;
        whole.0 + whole.1
    };
    let first = capture();
    let second = capture();
    first + second + a.0 + b.1
}

fn main() {
    unsafe {
        printf(c"%d\n".as_ptr(), evaluate(3));
    }
}
