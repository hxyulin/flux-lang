//! `match` on enums, integers and tuples: literal, range and or-patterns,
//! guards (`if let` ones too), `if let`, `while let` and `let`-`else`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

enum Shape {
    Circle(i64),
    Rect { w: i64, h: i64 },
    Empty,
}

fn area(s: &Shape) -> i64 {
    match s {
        Shape::Circle(r) => 3 * *r * *r,
        Shape::Rect { w, h } => *w * *h,
        Shape::Empty => 0,
    }
}

enum Opt {
    Some(i64),
    None,
}

use Opt::*;

fn first_even(a: i64, b: i64) -> Opt {
    if a % 2 == 0 {
        Some(a)
    } else if b % 2 == 0 {
        Some(b)
    } else {
        None
    }
}

fn classify(x: i32) -> i64 {
    match x {
        -2147483648..=-1 => -1,
        0 => 0,
        1 | 2 | 3 => 1,
        4..10 => 2,
        _ => 3,
    }
}

fn main() {
    show(area(&Shape::Circle(2)));
    show(area(&Shape::Rect { w: 3, h: 4 }));
    show(area(&Shape::Empty));
    if let Some(x) = first_even(3, 8) {
        show(x);
    }
    let Some(y) = first_even(6, 1) else {
        return;
    };
    show(y);
    match first_even(1, 3) {
        Some(_) => show(100),
        None => show(200),
    }
    for_each();
    show(classify(-5));
    show(classify(0));
    show(classify(2));
    show(classify(7));
    show(classify(70));
    show(guarded(Some(8)));
    show(guarded(Some(2)));
    show(guarded(Some(3)));
    show(guarded(None));
    let same = Shape::Circle(5) as Shape;
    show(area(&same));
}

fn half(x: i64) -> Opt {
    if x % 2 == 0 { Some(x / 2) } else { None }
}

fn guarded(o: Opt) -> i64 {
    match o {
        Some(x) | Some(x) if let Some(h) = half(x) && h > 1 => h,
        Some(x) if let Some(_) = half(x + 1) => -x,
        Some(_) => 100,
        None => 0,
    }
}

fn for_each() {
    let mut n = 0;
    let mut acc = 0;
    while let Some(v) = first_even(n, n + 1) {
        acc += v;
        n += 1;
        if n > 5 {
            break;
        }
    }
    show(acc);
}
