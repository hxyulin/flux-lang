//! Slice patterns' elements as places of their own, as rustc's: an
//! element from each end of a `&mut [T]` and the rest between them
//! borrowed mutably at once, elements and the rest of an array moved
//! out of it, and what is left of the array dropped in rustc's order
//! (runs of elements without patterns of their own, last first).

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

struct Loud(i32);

impl Drop for Loud {
    fn drop(&mut self) {
        show(900, self.0 as i64);
    }
}

fn swap_ends(s: &mut [i32]) {
    if let [first, .., last] = s {
        core::mem::swap(first, last);
    }
}

fn bump_rest(s: &mut [i32]) {
    if let [_, rest @ ..] = s {
        for x in rest.iter_mut() {
            *x += 10;
        }
    }
}

fn split(a: &mut [i32; 4]) -> i32 {
    let [first, rest @ ..] = a;
    *first += 1;
    rest[0] += 2;
    let [x, mid @ .., y] = rest;
    *x + mid.len() as i32 * 100 + *y * 1000
}

fn take_two(arr: [Loud; 3]) -> i32 {
    let [a, _, c] = arr;
    show(1, a.0 as i64);
    a.0 + c.0
}

fn first_moved(arr: [Loud; 3]) {
    let [a, ..] = arr;
    show(2, a.0 as i64);
}

fn rest_moved(arr: [Loud; 4]) -> i32 {
    let [first, rest @ ..] = arr;
    show(3, first.0 as i64);
    rest.len() as i32
}

fn conditional(arr: [Loud; 2], keep: bool) {
    if keep {
        let [a, b] = arr;
        show(4, (a.0 + b.0) as i64);
    }
    show(5, 0);
}

fn sum_place(v: &[i32]) -> i32 {
    match *v {
        [a, b, ..] => a + b,
        [a] => a,
        [] => 0,
    }
}

fn main() {
    show(16, sum_place(&[3, 4, 5]) as i64 + sum_place(&[7]) as i64 * 100);
    let mut v = [1, 2, 3, 4];
    swap_ends(&mut v);
    show(10, (v[0] * 10 + v[3]) as i64);
    bump_rest(&mut v);
    show(11, v.iter().sum::<i32>() as i64);
    let mut w = [1, 2, 3, 4];
    show(12, split(&mut w) as i64);
    show(13, w.iter().sum::<i32>() as i64);
    show(14, take_two([Loud(1), Loud(2), Loud(3)]) as i64);
    first_moved([Loud(4), Loud(5), Loud(6)]);
    show(15, rest_moved([Loud(7), Loud(8), Loud(9), Loud(10)]) as i64);
    conditional([Loud(11), Loud(12)], true);
    conditional([Loud(13), Loud(14)], false);
}
