//! Function pointers in structs that derive `Clone` and `Copy`, copied,
//! cloned and called.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}
#[derive(Clone, Copy)]
struct Op {
    f: fn(i32) -> i32,
    g: fn(u8, u8) -> u8,
}
#[derive(Clone)]
struct Holder {
    f: fn() -> i32,
}
fn one() -> i32 {
    1
}
fn inc(x: i32) -> i32 {
    x + 1
}
fn add(a: u8, b: u8) -> u8 {
    a + b
}
fn main() {
    let op = Op { f: inc, g: add };
    let copy = op;
    let h = Holder { f: one };
    let k = h.clone();
    let f2 = op.f.clone();
    unsafe {
        printf(c"%d %d %d %d\n".as_ptr(), (copy.f)(1), (op.g)(2, 3) as i32, (k.f)(), f2(5));
    }
}
