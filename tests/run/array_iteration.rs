//! Iterating arrays by value (`for x in array`, `array.into_iter()`),
//! by shared and mutable reference (`for x in &mut array`,
//! `iter_mut`): an iterator by value moves the elements out one by one
//! and drops those it has not given when it is dropped.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {
        unsafe { printf(c"drop %d\n".as_ptr(), self.0) };
    }
}

fn total<I: IntoIterator<Item = i32>>(items: I) -> i32 {
    let mut sum = 0;
    for x in items {
        sum += x;
    }
    sum
}

fn count<I: IntoIterator>(items: I) -> i32 {
    let mut n = 0;
    for _ in items {
        n += 1;
    }
    n
}

#[derive(Clone, Copy)]
struct P {
    x: i32,
    y: i32,
}

fn noisy(base: i32) {
    let all = [Noisy(base), Noisy(base + 1), Noisy(base + 2), Noisy(base + 3)];
    let mut iter = all.into_iter();
    let first = iter.next().unwrap();
    let last = iter.next_back().unwrap();
    unsafe { printf(c"first %d last %d left %d\n".as_ptr(), first.0, last.0, iter.as_slice().len() as i32) };
    drop(iter);
    unsafe { printf(c"iterator dropped\n".as_ptr()) };
    for n in [Noisy(base + 10), Noisy(base + 11)] {
        unsafe { printf(c"got %d\n".as_ptr(), n.0) };
    }
    for n in [Noisy(base + 20), Noisy(base + 21), Noisy(base + 22)] {
        if n.0 == base + 21 {
            break;
        }
    }
    let moved = [Noisy(base + 30), Noisy(base + 31)];
    let mut iter = moved.into_iter();
    let kept = iter.next();
    let _ = kept.is_some();
}

fn main() {
    let a = [1, 2, 3, 4];
    let mut b = [10, 20, 30, 40];
    let doubled: i32 = a.into_iter().map(|x| x * 2).sum();
    for x in &mut b {
        *x += 1;
    }
    for x in (&mut b).into_iter() {
        *x *= 2;
    }
    let mut via_ref = 0;
    for x in &b {
        via_ref += *x;
    }
    let grid = [[1, 2], [3, 4], [5, 6]];
    let mut digits = 0;
    for row in grid {
        for x in row {
            digits = digits * 10 + x;
        }
    }
    let points = [P { x: 1, y: 2 }, P { x: 3, y: 4 }];
    let mut products = 0;
    for P { x, y } in points {
        products += x * y;
    }
    let back = [5, 6, 7].into_iter().rev().fold(0, |acc, x| acc * 10 + x);
    let mut m = [1, 2, 3];
    for x in m.iter_mut() {
        *x += 10;
    }
    let mut reversed = 0;
    for x in m.iter_mut().rev() {
        reversed = reversed * 100 + *x;
    }
    unsafe {
        printf(
            c"%d %d %d %d %d %d %d %d %d %d\n".as_ptr(),
            total(a),
            doubled,
            count(&a),
            (&a).into_iter().count() as i32,
            via_ref,
            digits,
            products,
            back,
            reversed,
            b[3],
        );
    }
    noisy(100);
}
