//! Arrays compared with slices and references to them, either way round
//! (`s == [1, 2]`, `[1, 2] == *s`, `w == b"fn"`), through the compiler's
//! impls, which compare them as slices; and slices with slices.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn check(s: &[i32], t: &mut [i32]) -> i32 {
    let a = [1, 2, 3];
    let mut n = 0;
    if s == a {
        n += 1;
    }
    if a == s {
        n += 10;
    }
    if a == *s {
        n += 100;
    }
    if s[..] == a {
        n += 1000;
    }
    if t != a {
        n += 10000;
    }
    if a[..2] == s[..2] {
        n += 100000;
    }
    if &a[..] == s {
        n += 1000000;
    }
    n
}

fn arrays_and_slices() {
    let x = [1, 2, 3];
    let mut y = [1, 2, 4];
    let word: &[u8] = &[104, 105];
    let is_hi = word == [104u8, 105];
    let not = word != [104u8, 106];
    unsafe {
        printf(c"%d %d %d %d\n".as_ptr(), check(&x, &mut y), check(&[1, 2], &mut [1, 2, 3]), is_hi as i32, not as i32);
    }
}

fn is_word(w: &[u8]) -> bool {
    w == b"let" || w == b"fn"
}

fn main() {
    arrays_and_slices();
    let a: &[u8] = b"fn";
    let b = [1, 2];
    let c: &[i32] = &[1, 2];
    unsafe {
        printf(
            c"%d %d %d %d %d\n".as_ptr(),
            is_word(a) as i32,
            is_word(b"if") as i32,
            (&b == c) as i32,
            (c == &b) as i32,
            (b"ab" == b"ab") as i32,
        );
    }
}
