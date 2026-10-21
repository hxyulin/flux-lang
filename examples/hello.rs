//! Prints a greeting through libc, since flux has no std.

unsafe extern "C" {
    fn puts(s: *const u8) -> i32;
}

/// A value that may be missing.
enum Maybe<T> {
    Just(T),
    Nothing,
}

fn first_even(values: &[i64]) -> Maybe<i64> {
    let mut i = 0;
    while i < values.len() {
        if values[i] % 2 == 0 {
            return Maybe::Just(values[i]);
        }
        i += 1;
    }
    Maybe::Nothing
}

fn main() {
    let found = match first_even(&[3, 5, 8, 9]) {
        Maybe::Just(n) => n,
        Maybe::Nothing => -1,
    };
    if found > 0 {
        unsafe { puts(c"hello from flux".as_ptr().cast()) };
    }
}
