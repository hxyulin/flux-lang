// Closures, ranges and `for` loops: closures see the bindings around
// them but not the loops; ranges are `core`'s structs; `for` loops are
// `match`es of `IntoIterator::into_iter` around a `loop`.

fn closures(k: i32) -> i32 {
    let add = |a, b: i32| a + b + k;
    let typed = move |x: i32| -> i32 { x * 2 };
    let unit = || {};
    let pair = |(a, b): (i32, i32)| a - b;
    unit();
    add(1, 2) + typed(3) + pair((4, 5))
}

fn nested() -> i32 {
    let base = 1;
    let outer = |x: i32| {
        let inner = |y: i32| y + base;
        inner(x)
    };
    outer(2)
}

fn ranges() {
    let _a = 0..10;
    let _b = 1..=5;
    let _c = 3..;
    let _d = ..4;
    let _e = ..=7;
    let _f = ..;
}

fn loops() -> i32 {
    let mut total = 0;
    for i in 0..3 {
        total += i;
    }
    'outer: for i in 0..10 {
        for j in 0..i {
            if j == 3 {
                continue 'outer;
            }
            if i == 8 {
                break 'outer;
            }
        }
    }
    'search: loop {
        while total > 0 {
            total -= 1;
            if total == 2 {
                break 'search;
            }
        }
    }
    let closure_with_loop = || {
        for _ in 0..2 {
            break;
        }
    };
    closure_with_loop();
    total
}

fn main() {}
