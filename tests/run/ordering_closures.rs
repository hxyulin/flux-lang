//! Comparisons that take closures: `Ordering::then_with`, `max_by`,
//! `min_by_key` and the `Iterator` methods comparing by a closure or a
//! key, and `Iterator::eq`.

use core::cmp::Ordering;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

fn main() {
    let o = 1.cmp(&1).then_with(|| 3.cmp(&2));
    show(1, o as i32);
    show(2, core::cmp::max_by(3, -5, |a: &i32, b: &i32| a.abs().cmp(&b.abs())));
    show(3, core::cmp::min_by_key(-3, 2, |x: &i32| x.abs()));
    show(4, (1..10).max_by(|a, b| (a % 4).cmp(&(b % 4))).unwrap());
    show(5, (1..10).min_by(|a, b| (a % 4).cmp(&(b % 4))).unwrap());
    show(6, (1..10).max_by_key(|x| x % 5).unwrap());
    let ord = (1..4).map(|x| x * 2).eq(2..7);
    show(7, ord as i32);
    let _ = Ordering::Less;
}
