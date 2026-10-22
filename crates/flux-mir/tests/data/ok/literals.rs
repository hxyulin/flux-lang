const BYTES: &[u8] = b"ab";
const NEXT: char = 'b';

fn kind(c: char) -> u8 {
    match c {
        'a' => 1,
        NEXT..='z' => 2,
        _ => 3,
    }
}

fn len(s: &[u8]) -> usize {
    s.len()
}

fn main() {
    let c = '\u{e9}';
    let code = c as u32 + b'x' as u32;
    let back = b'y' as char;
    let k = kind(back);
    let n = len(b"xyz") + len(BYTES);
}
