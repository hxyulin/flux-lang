//! Byte literals and byte strings: escapes, raw byte strings, byte
//! strings in statics and constants coerced to slices, matching bytes
//! and byte ranges, and the ASCII methods of `u8` and `[u8]`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn putchar(c: i32) -> i32;
}

static TEXT: &[u8] = b"Hello,\tWorld!\n\xff";
const RAW: &[u8; 5] = br#"a\n"b"#;
const SHOUT: u8 = b'q'.to_ascii_uppercase();
const LETTERS: usize = b"abc".len();

fn count(s: &[u8], b: u8) -> i32 {
    let mut n = 0;
    for &x in s {
        if x == b {
            n += 1;
        }
    }
    n
}

fn classify(b: u8) -> u8 {
    match b {
        b'a'..=b'z' => b'l',
        b'A'..=b'Z' => b'U',
        b'0'..=b'9' => b'd',
        b' ' | b'\t' | b'\n' => b'_',
        b'\'' | b'"' | b'\\' => b'q',
        0x80.. => b'h',
        _ => b'.',
    }
}

fn put(s: &[u8]) {
    for &b in s {
        unsafe { putchar(b as i32) };
    }
    unsafe { putchar(b'\n' as i32) };
}

fn flags(b: u8) -> i32 {
    let mut f = 0;
    let tests = [
        b.is_ascii(),
        b.is_ascii_alphabetic(),
        b.is_ascii_uppercase(),
        b.is_ascii_lowercase(),
        b.is_ascii_alphanumeric(),
        b.is_ascii_digit(),
        b.is_ascii_hexdigit(),
        b.is_ascii_punctuation(),
        b.is_ascii_graphic(),
        b.is_ascii_whitespace(),
        b.is_ascii_control(),
    ];
    let mut i = 0;
    while i < tests.len() {
        if tests[i] {
            f |= 1 << i;
        }
        i += 1;
    }
    f
}

fn main() {
    let s: &[u8] = b"abc, ABC; 123 'q' \"x\" \\";
    let a = b"xyz";
    unsafe {
        printf(c"%d %d %d\n".as_ptr(), count(s, b'b'), s.len() as i32, a.len() as i32);
        printf(c"%d %d %d\n".as_ptr(), TEXT.len() as i32, TEXT[14] as i32, RAW[1] as i32);
        printf(c"%c %d\n".as_ptr(), SHOUT as i32, LETTERS as i32);
    }
    let mut kinds = [b'-'; 32];
    let mut i = 0;
    while i < s.len() {
        kinds[i] = classify(s[i]);
        i += 1;
    }
    kinds[i] = classify(TEXT[14]);
    put(&kinds);
    put(RAW);
    put(b"multi\
           line");
    let all: &[u8] = b"\x00\x09\x0a\x0c\x0d\x1f !09:@AFGZ[`afgz{~\x7f\x80\xff";
    for &b in all {
        unsafe { printf(c"%02x:%03x ".as_ptr(), b as i32, flags(b)) };
    }
    unsafe { printf(c"\n".as_ptr()) };
    let mut word = *b"MiXeD 42!";
    let w: &mut [u8] = &mut word;
    w.make_ascii_lowercase();
    put(w);
    w.make_ascii_uppercase();
    put(w);
    let hello: &[u8] = b"Hello";
    let mut one = b'k';
    one.make_ascii_uppercase();
    unsafe {
        printf(
            c"%c %c %c %d %d\n".as_ptr(),
            one as i32,
            b'Z'.to_ascii_lowercase() as i32,
            b'5'.to_ascii_uppercase() as i32,
            b'a'.eq_ignore_ascii_case(&b'A') as i32,
            b'a'.eq_ignore_ascii_case(&b'b') as i32,
        );
        printf(
            c"%d %d %d %d %d\n".as_ptr(),
            s.is_ascii() as i32,
            TEXT.is_ascii() as i32,
            hello.eq_ignore_ascii_case(b"hELLO") as i32,
            hello.eq_ignore_ascii_case(b"Help!") as i32,
            hello.eq_ignore_ascii_case(b"Hell") as i32,
        );
    }
}
