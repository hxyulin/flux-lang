//! Closures that capture arrays and slices: elements read and written
//! through captured arrays, slices captured by reference, arrays moved
//! into closures, and closures indexing in iterator adapters.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

fn sum_by<F: Fn(usize) -> i32>(n: usize, f: F) -> i32 {
    let mut total = 0;
    for i in 0..n {
        total += f(i);
    }
    total
}

fn main() {
    let a = [1, 2, 3, 4];
    show(1, sum_by(4, |i| a[i] * 10));
    let mut b = [0; 3];
    let mut set = |i: usize, v: i32| b[i] = v;
    set(0, 5);
    set(2, 7);
    show(2, b[0] + b[1] + b[2]);
    let s: &[i32] = &a;
    let total: i32 = (0..s.len()).map(|i| s[i]).sum();
    show(3, total);
    let owned = move || a[3];
    show(4, owned());
    let arr = [[1, 2], [3, 4]];
    show(5, (0..2).map(|i| arr[i][1 - i]).fold(0, |x, y| x * 10 + y));
}
