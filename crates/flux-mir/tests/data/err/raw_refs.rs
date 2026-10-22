// `&raw mut` of a place that may not be borrowed mutably (E0596): a
// binding without `mut`, and a place behind a `&` reference.

fn f() -> i32 {
    1
}

fn main() {
    let x = 5;
    let _a = &raw mut x;
    let r = &x;
    let _c = &raw mut *r;
}
