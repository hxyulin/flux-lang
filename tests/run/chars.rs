//! `char`: literals with escapes, comparisons, casts to and from
//! integers, matching on chars and char ranges, constants, `char` in
//! structs and arrays, and `core`'s methods that need no Unicode
//! tables.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const NEWLINE: char = '\n';
const SMILE: char = '\u{1F600}';
const SHOUT: char = 'q'.to_ascii_uppercase();
const SEVEN: Option<u32> = '7'.to_digit(10);
const EURO: Option<char> = char::from_u32(0x20AC);

fn label(name: &core::ffi::CStr, value: u32) {
    unsafe { printf(c"%s=%u\n".as_ptr(), name.as_ptr(), value) };
}

fn from(code: u32) -> u32 {
    match char::from_u32(code) {
        Some(c) => c as u32,
        None => 0xFFFF_FFFF,
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
struct Tagged {
    tag: char,
    value: i32,
}

fn kind(c: char) -> i32 {
    match c {
        'a'..='z' => 1,
        'A'..='Z' => 2,
        '0'..='9' => 3,
        ' ' | '\t' | NEWLINE => 4,
        '\'' | '"' | '\\' => 5,
        '\u{80}'..='\u{7FF}' => 6,
        '\u{800}'..='\u{FFFF}' => 7,
        '\u{10000}'.. => 8,
        _ => 0,
    }
}

fn show(c: char) {
    unsafe {
        printf(
            c"%u %d %d %d %d\n".as_ptr(),
            c as u32,
            kind(c),
            c.len_utf8() as i32,
            c.is_whitespace() as i32,
            c.is_ascii() as i32,
        );
    }
}

fn digit(c: char, radix: u32) -> i64 {
    match c.to_digit(radix) {
        Some(d) => d as i64,
        None => -1,
    }
}

fn main() {
    let letters = ['a', 'Z', '5', ' ', '\'', '\\', 'é', '€', SMILE, '\0', '\x7f', '\u{3000}'];
    for &c in &letters {
        show(c);
    }
    let a = 'a';
    let b = 'b';
    unsafe {
        printf(
            c"%d %d %d %d %d %d\n".as_ptr(),
            (a < b) as i32,
            (a == 'a') as i32,
            (a != b) as i32,
            (b >= a) as i32,
            ('é' > 'z') as i32,
            (a.max(b) as u32) as i32,
        );
        printf(c"%c%c%c\n".as_ptr(), 72u8 as char as i32, 'i' as i32, '!' as u8 as i32);
        printf(c"%c %u %d\n".as_ptr(), SHOUT as i32, NEWLINE as u32, SEVEN.unwrap_or(0) as i32);
        printf(
            c"%d %d %d %d %d %d\n".as_ptr(),
            'é' as u8 as i32,
            SMILE as u16 as i32,
            SMILE as i8 as i32,
            255u8 as char as u32 as i32,
            char::MAX as u32 as i32,
            char::REPLACEMENT_CHARACTER as i32,
        );
        printf(
            c"%lld %lld %lld %lld %lld %lld\n".as_ptr(),
            digit('7', 10),
            digit('f', 16),
            digit('F', 16),
            digit('z', 36),
            digit('g', 16),
            digit('é', 36),
        );
    }
    let mut i = 0;
    while i < 40 {
        let shown = match char::from_digit(i, 36) {
            Some(c) => c,
            None => '?',
        };
        unsafe { printf(c"%c".as_ptr(), shown as i32) };
        i += 1;
    }
    unsafe { printf(c"\n".as_ptr()) };
    let all = ['\t', 'A', 'm', '0', '~', '[', 'F', 'g', '\x0c', '\x0b', ' '];
    for &c in &all {
        let flags = [
            c.is_ascii_alphabetic(),
            c.is_ascii_uppercase(),
            c.is_ascii_lowercase(),
            c.is_ascii_alphanumeric(),
            c.is_ascii_digit(),
            c.is_ascii_hexdigit(),
            c.is_ascii_punctuation(),
            c.is_ascii_graphic(),
            c.is_ascii_whitespace(),
            c.is_ascii_control(),
            c.is_digit(16),
        ];
        let mut bits = 0;
        let mut k = 0;
        while k < flags.len() {
            if flags[k] {
                bits |= 1 << k;
            }
            k += 1;
        }
        unsafe {
            printf(
                c"%u:%03x %c%c\n".as_ptr(),
                c as u32,
                bits,
                c.to_ascii_uppercase() as i32,
                c.to_ascii_lowercase() as i32,
            )
        };
    }
    for &code in &[0x41, 0xD7FF, 0xD800, 0xDFFF, 0xE000, 0x10FFFF, 0x110000] {
        label(c"from_u32", from(code));
    }
    label(c"euro", EURO.unwrap_or('?') as u32);
    let mut alphabet = 0;
    for c in 'a'..='z' {
        alphabet += c as u32 - 'a' as u32;
    }
    let mut across = 0;
    for _ in '\u{D7FE}'..'\u{E002}' {
        across += 1;
    }
    let last = ('a'..'e').rev().next().unwrap_or('?');
    label(c"alphabet", alphabet);
    label(c"across", across);
    label(c"last", last as u32);
    let mut m = 'x';
    m.make_ascii_uppercase();
    let x = Tagged { tag: 'x', value: 1 };
    let y = Tagged { tag: 'y', value: 0 };
    unsafe {
        printf(
            c"%c %d %d %d %d\n".as_ptr(),
            m as i32,
            'A'.eq_ignore_ascii_case(&'a') as i32,
            (x < y) as i32,
            (x == x) as i32,
            u32::from('A') as i32 + char::from(b'B') as i32,
        );
    }
}
