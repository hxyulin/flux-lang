//! Arrays of function pointers: annotated, and from function items of
//! different functions, which coerce to their pointer type; indexed and
//! called.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}
fn double(x: i32) -> i32 {
    x * 2
}
fn negate(x: i32) -> i32 {
    -x
}
fn main() {
    let ops: [fn(i32) -> i32; 2] = [double, negate];
    let mut total = 0;
    let mut i = 0;
    while i < 2 {
        total += ops[i](5);
        i += 1;
    }
    let mixed = [double, negate];
    unsafe {
        printf(c"%d %d\n".as_ptr(), total, mixed[1](3));
    }
}
