//! Miscompilation: a tuple holding a `&[T; N]` coerced to `&[T]`, as an
//! element of an array whose type says `&[T]`, is built in a temporary
//! of the uncoerced type `(i32, &[i32; 5])` (see `flux mir`), then copied
//! into the array of `(i32, &[i32])`: the slice's length is garbage.
//! rustc prints 5; flux prints a different large number each build.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn main() {
    let a = [1, 2, 3, 4, 5];
    let table: [(i32, &[i32]); 1] = [(10, &a)];
    unsafe {
        printf(c"%zu\n".as_ptr(), table[0].1.len());
    }
}
