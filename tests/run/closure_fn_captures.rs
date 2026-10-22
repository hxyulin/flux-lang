//! Closures that call captured locals holding functions: function
//! pointers and function items, captured by reference or by `move`,
//! from nested closures, and closures called directly.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

fn is_odd(x: &u8) -> bool {
    *x % 2 == 1
}

fn triple(x: i32) -> i32 {
    x * 3
}

fn make_adder(f: fn(i32) -> i32) -> impl Fn(i32) -> i32 {
    move |x| f(x) + 1
}

fn main() {
    let pred: fn(&u8) -> bool = is_odd;
    show(1, (0..10u8).filter(|b| pred(b)).count() as i32);
    let item = is_odd;
    show(2, (0..7u8).filter(|b| item(b)).count() as i32);
    let f = |b: &u8| pred(b);
    show(3, f(&3) as i32 + f(&4) as i32);
    let g = triple;
    let outer = |x: i32| {
        let inner = |y: i32| g(y) + g(x);
        inner(1)
    };
    show(4, outer(5));
    let add = make_adder(triple);
    show(5, add(4));
    let mut h: fn(i32) -> i32 = triple;
    let mut calls = 0;
    let mut call = |x: i32| {
        calls += 1;
        h(x)
    };
    show(6, call(2) + call(3));
    show(7, calls);
    h = |x| x - 1;
    show(8, h(10));
}
