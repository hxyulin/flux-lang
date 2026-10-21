unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn takes_bytes(p: *const u8);
}

fn variadic_args(small: u8, flag: bool, wide: i16) {
    unsafe {
        printf(c"%d".as_ptr(), small);
        printf(c"%d".as_ptr(), flag);
        printf(c"%d".as_ptr(), wide);
        printf(c"%d".as_ptr(), 1i64, 2u32);
        printf();
    }
}

fn methods(x: i32) {
    let s = c"text";
    let a = s.as_ptr(1);
    let b = s.to_bytes();
    let c = x.abs();
}

fn pointers() {
    unsafe {
        takes_bytes(c"text".as_ptr());
    }
    let p = c"text".as_ptr();
    let n = p as usize;
}
