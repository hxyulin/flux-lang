//! A closure that calls a captured local holding a function (a function
//! pointer or a function item): `|b| pred(b)`.
//!
//! rustc: compiles; exits with status 5.
//! flux: "error: internal compiler error: a local neither declared nor
//! captured" at `pred` inside the closure. The capture analysis seems to
//! miss a local that is only used as a callee. Also with `let pred =
//! is_odd;` (a function item) and with a closure called directly
//! (`let f = |b: &u8| pred(b); f(&3)`). Found in round2/matchers.rs.

fn is_odd(x: &u8) -> bool {
    *x % 2 == 1
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let pred: fn(&u8) -> bool = is_odd;
    let n = (0..10u8).filter(|b| pred(b)).count();
    unsafe { exit(n as i32) }
}
