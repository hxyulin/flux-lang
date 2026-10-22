//! Closures are `Clone` where they are `Copy`, and cloned by a copy:
//! directly and through a generic `Clone` bound.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn twice<F: Fn(i32) -> i32 + Clone>(f: F) -> i32 {
    let g = f.clone();
    f(1) + g(2)
}

fn main() {
    let k = 10;
    let c = |x: i32| x + k;
    let d = c.clone();
    unsafe {
        printf(c"%d %d\n".as_ptr(), d(5), twice(c));
    }
}
