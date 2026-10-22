fn by_value(s: str) {}

fn mismatches(s: &str) {
    let a: &str = b"bytes";
    let b: &[u8] = "text";
    let c: char = "c";
    let d = s + "more";
    let e = s.len() + 'a';
    let f = s == 'a';
    let g = s.starts_with(1);
}

fn index(s: &str) -> u8 {
    s[0]
}

fn main() {}
