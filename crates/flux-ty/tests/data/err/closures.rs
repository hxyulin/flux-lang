// Closures that do not fit what they are passed to, calls of values
// that are not functions, and what closures cannot do.

struct S;

fn apply<F: Fn(i32) -> i32>(f: F) -> i32 {
    f(1)
}

fn only_fn<F: Fn()>(f: F) {
    f();
}

fn not_fn<T>(t: T) {
    t(1);
}

fn arity() {
    apply(|a, b| a + b);
}

fn mismatch() {
    apply(|x: u8| 1);
}

fn ret_mismatch() {
    apply(|x| true);
}

fn call_args() {
    let f = |x: i32| x;
    f(1, 2);
}

fn not_callable() {
    let x = 5;
    x();
}

fn unknown() {
    let f = |x| x.0;
}

fn wrong_arg_type() {
    let f = |x: i32| x;
    f(true);
}

fn wrong_kind() {
    let s = S;
    let c = || drop(s);
    only_fn(c);
}

fn bad_sugar<T: Clone(i32)>(t: T) {}

fn unstable_form<F: Fn<(i32,)>>(f: F) {}

fn not_iterator() {
    for x in 5 {}
    for s in S {}
}

fn refutable() {
    for Some(x) in (0..3).map(|i| Some(i)) {}
}

fn for_body() {
    for i in 0..3 {
        5
    }
}

fn for_break() {
    for i in 0..3 {
        break 5;
    }
}

fn bad_range() {
    let r = 1..true;
}

fn by_name() {
    let f = |x: i32| x;
    let a = f.call((1,));
    let b = Fn::call(&f, (1,));
}

fn main() {}
