//! Compiler panic: the same unsizing coercion inside a tuple nested in a
//! tuple makes `flux build` panic in keel's frontend:
//! "assertion `left == right` failed: var2 has type i64, assigned a i32".
//! rustc prints 5.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn main() {
    let a = [1, 2, 3, 4, 5];
    let nested: ((i32, &[i32]), i32) = ((1, &a), 2);
    unsafe {
        printf(c"%zu\n".as_ptr(), (nested.0).1.len());
    }
}
