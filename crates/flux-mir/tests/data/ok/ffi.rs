//! C strings, raw pointers and variadic declarations.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn strlen(s: *const i8) -> usize;
    fn fill_bytes(dest: *mut u8, byte: i32, count: usize) -> *mut u8;
}

fn length() -> usize {
    let text: *const i8 = c"caf\u{e9}\t\"x\"".as_ptr();
    unsafe { strlen(text) }
}

fn print(value: i64) {
    unsafe {
        printf(c"%lld %s\n".as_ptr(), value, cr#"raw "text""#.as_ptr());
    }
}
