const LETTER: char = 'x';
static BYTES: &[u8] = b"bytes\n";

fn classify(c: char) -> u8 {
    match c {
        'a'..='z' | 'A'..='Z' => b'a',
        '0'..='9' => b'0',
        LETTER => b'x',
        '\u{80}'.. => 0x80,
        _ => b'?',
    }
}

fn exhaustive(c: char) -> bool {
    match c {
        '\0'..='\u{D7FF}' => true,
        '\u{E000}'..='\u{10FFFF}' => false,
    }
}

fn main() {
    let c = 'é';
    let code = c as u32;
    let byte = 65 as char;
    let raw = br#"a"b"#;
    let slice: &[u8] = raw;
    let digit = '7'.to_digit(10);
    let upper = 'q'.to_ascii_uppercase();
    let same = c == 'é' && byte < upper;
    let kind = classify(c);
    let e = exhaustive(c);
    let n = BYTES.len() + slice.len() + c.len_utf8();
}
