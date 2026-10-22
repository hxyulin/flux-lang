//! `str`: string literals with escapes, raw strings, `&str` in
//! constants, statics, structs and arrays, `len`, `as_bytes`, `bytes`,
//! `chars` and `char_indices` (from both ends) over UTF-8,
//! comparisons, `starts_with`, `ends_with`, `contains` and `find` with
//! `&str` and `char` patterns, string literal patterns,
//! `str::from_utf8` and its errors, and printing with `printf`'s `%.*s`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const GREETING: &str = "Grüße, 世界! 😀";
static RAW: &str = r#"raw "quoted" \n"#;
const LEN: usize = "héllo".len();
const EMPTY: bool = "".is_empty();

struct Named<'a> {
    name: &'a str,
    value: i32,
}

fn print(s: &str) {
    unsafe { printf(c"[%.*s]\n".as_ptr(), s.len() as i32, s.as_ptr()) };
}

fn number(label: &str, n: i64) {
    unsafe { printf(c"%.*s=%lld\n".as_ptr(), label.len() as i32, label.as_ptr(), n) };
}

fn op(name: &str) -> i64 {
    match name {
        "add" | "plus" => 1,
        "sub" => 2,
        "" => 3,
        _ => 0,
    }
}

fn nested(pair: (&str, i64)) -> i64 {
    match pair {
        ("x", n) if n > 0 => n,
        ("x", _) => -1,
        (_, n) => n * 10,
    }
}

fn position(found: Option<usize>) -> i64 {
    match found {
        Some(at) => at as i64,
        None => -1,
    }
}

fn check(bytes: &[u8]) {
    match str::from_utf8(bytes) {
        Ok(s) => {
            print(s);
            number("chars", s.chars().count() as i64);
        }
        Err(e) => {
            let len = match e.error_len() {
                Some(n) => n as i64,
                None => -1,
            };
            number("valid up to", e.valid_up_to() as i64);
            number("error len", len);
        }
    }
}

fn main() {
    print("hello\tworld\n\\\"escaped\"\u{e9}");
    print(GREETING);
    print(RAW);
    print("continued \
           line");
    number("len", GREETING.len() as i64);
    number("LEN", LEN as i64);
    number("EMPTY", EMPTY as i64);
    number("chars", GREETING.chars().count() as i64);

    let mut sum = 0;
    for b in "abc".bytes() {
        sum += b as i64;
    }
    number("bytes", sum);
    let back: i64 = "xyz".bytes().rev().next().unwrap_or(0) as i64;
    number("last byte", back);

    for c in GREETING.chars() {
        unsafe { printf(c"%u ".as_ptr(), c as u32) };
    }
    unsafe { printf(c"\n".as_ptr()) };
    for c in GREETING.chars().rev() {
        unsafe { printf(c"%u ".as_ptr(), c as u32) };
    }
    unsafe { printf(c"\n".as_ptr()) };
    for (i, c) in "aé😀z".char_indices() {
        unsafe { printf(c"%zu:%u ".as_ptr(), i, c as u32) };
    }
    for (i, c) in "aé😀z".char_indices().rev() {
        unsafe { printf(c"%zu:%u ".as_ptr(), i, c as u32) };
    }
    unsafe { printf(c"\n".as_ptr()) };

    let words = ["apple", "Apple", "apples", "banana", "", "äpple"];
    for &a in &words {
        for &b in &words {
            unsafe { printf(c"%d%d%d ".as_ptr(), (a == b) as i32, (a < b) as i32, (a >= b) as i32) };
        }
        unsafe { printf(c"\n".as_ptr()) };
    }
    let hello = "hello, world";
    number("starts hello", hello.starts_with("hello") as i64);
    number("starts h", hello.starts_with('h') as i64);
    number("starts world", hello.starts_with("world") as i64);
    number("ends world", hello.ends_with("world") as i64);
    number("ends d", hello.ends_with('d') as i64);
    number("ends empty", hello.ends_with("") as i64);
    number("contains o, w", hello.contains("o, w") as i64);
    number("contains z", hello.contains('z') as i64);
    number("find world", position(hello.find("world")));
    number("find l", position(hello.find('l')));
    number("find é", position(GREETING.find('界')));
    number("find none", position(hello.find("xyz")));
    number("ascii", hello.is_ascii() as i64);
    number("not ascii", GREETING.is_ascii() as i64);
    number("ignore case", "HeLLo".eq_ignore_ascii_case("hello") as i64);
    number("boundary 3", GREETING.is_char_boundary(3) as i64);
    number("boundary 4", GREETING.is_char_boundary(4) as i64);

    let entries = [Named { name: "one", value: 1 }, Named { name: "two", value: 2 }];
    for e in &entries {
        number(e.name, e.value as i64);
    }
    for &name in &["add", "plus", "sub", "", "mul", "ad", "addd"] {
        number(name, op(name));
    }
    number("nested", nested(("x", 3)) + nested(("x", -3)) + nested(("y", 2)));
    if let "hello, world" = hello {
        number("if let", 1);
    }
    let bytes = hello.as_bytes();
    number("as_bytes", bytes[7] as i64);

    check(b"plain ascii");
    check("ünïcödé".as_bytes());
    check(b"bad \xff byte");
    check(b"truncated \xe4\xb8");
    check(b"overlong \xc0\x80");
    check(b"surrogate \xed\xa0\x80");
    check(b"four \xf0\x9f\x98");
    check(b"wrong \xf0\x9f\x41\x80");
    check(b"too big \xf4\x90\x80\x80");
}
