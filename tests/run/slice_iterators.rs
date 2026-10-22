//! Iterators over slices: `for x in slice` and `for x in &array`,
//! `iter()` with adapters and from the back, `first`, `last` and `get`
//! returning `Option<&T>`, and a reference to the largest element found
//! by iterating, returned out of the function.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

struct Point {
    x: i64,
    y: i64,
}

fn sum(v: &[i64]) -> i64 {
    let mut total = 0;
    for x in v {
        total += *x;
    }
    total
}

fn largest(v: &[i64]) -> Option<&i64> {
    let mut best = v.first()?;
    for x in v.iter() {
        if *x > *best {
            best = x;
        }
    }
    Some(best)
}

fn furthest(points: &[Point]) -> &Point {
    let mut best = &points[0];
    for p in points {
        if p.x * p.x + p.y * p.y > best.x * best.x + best.y * best.y {
            best = p;
        }
    }
    best
}

fn main() {
    let a = [3i64, 1, 4, 1, 5, 9, 2, 6];
    let s: &[i64] = &a;
    show(1, sum(s));
    show(2, *largest(s).unwrap());
    let empty: [i64; 0] = [];
    show(3, match largest(&empty) {
        Some(x) => *x,
        None => -1,
    });
    show(4, s.iter().filter(|x| **x % 2 == 0).count() as i64);
    let doubled: i64 = s.iter().map(|x| *x * 2).sum();
    show(5, doubled);
    show(6, *s.last().unwrap_or(&0));
    show(7, *s.iter().rev().next().unwrap());
    show(8, match s.get(20) {
        Some(x) => *x,
        None => -2,
    });
    let mut total = 0;
    for (i, x) in s.iter().enumerate() {
        total += i as i64 * *x;
    }
    show(9, total);
    let mut count = 0;
    for x in &a {
        if *x > 3 {
            count += 1;
        }
    }
    show(10, count);
    let points = [
        Point { x: 1, y: 2 },
        Point { x: -4, y: 3 },
        Point { x: 2, y: 2 },
    ];
    let far = furthest(&points);
    show(11, far.x * 10 + far.y);
    let pos = s.iter().position(|x| *x == 9).unwrap_or(99);
    show(12, pos as i64);
}
