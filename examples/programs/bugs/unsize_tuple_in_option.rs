//! Miscompilation: the same unsizing coercion inside a tuple passed to
//! `Some` (with `Option<(i32, &[i32])>` annotated) loses the slice's
//! length: rustc prints 5, flux prints 0.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn main() {
    let a = [1, 2, 3, 4, 5];
    let opt: Option<(i32, &[i32])> = Some((1, &a));
    unsafe {
        printf(c"%zu\n".as_ptr(), opt.unwrap().1.len());
    }
}
