//! C strings and variadic calls: `printf` with every integer width and
//! format, many arguments, escapes, raw C strings, and pointers passed
//! through Rust functions.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn puts(s: *const i8) -> i32;
    fn putchar(c: i32) -> i32;
}

/// Passes a pointer through a Rust function.
fn show(label: *const i8, value: i64) {
    unsafe {
        printf(c"%s=%lld\n".as_ptr(), label, value);
    }
}

fn pick(first: bool) -> *const i8 {
    if first { c"first".as_ptr() } else { c"second".as_ptr() }
}

fn widths(a: i32, b: u32, c: i64, d: u64, e: isize, f: usize) {
    unsafe {
        printf(c"%d %u %lld %llu %ld %lu\n".as_ptr(), a, b, c, d, e, f);
        printf(c"%x %X %o %c%c\n".as_ptr(), b, b, b, 70, 108);
    }
}

fn main() {
    let message = c"strings: \t tab, \\ backslash, \" quote, \x41\x42, caf\u{e9}";
    unsafe {
        puts(message.as_ptr());
        puts(cr#"raw: no \n escapes, "quotes" kept"#.as_ptr());
        puts(c"continued \
               line"
            .as_ptr());
        puts(c"".as_ptr());
    }
    widths(-2147483648, 4294967295, -9223372036854775808, 18446744073709551615, -1, 42);
    show(c"answer".as_ptr(), 42);
    show(c"negative".as_ptr(), -7);
    let label: *const i8 = pick(false);
    unsafe {
        printf(c"%s and %s\n".as_ptr(), pick(true), label);
        let count = printf(
            c"%d %d %d %d %d %d %d %d %d %d %d %d\n".as_ptr(),
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
        );
        printf(c"printed %d bytes\n".as_ptr(), count);
        putchar(10);
        printf(c"%d%%\n".as_ptr(), 100);
    }
}
