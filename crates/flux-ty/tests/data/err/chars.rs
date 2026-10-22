fn casts(c: char, u: u32, b: bool, i: i8) {
    let _ = u as char;
    let _ = b as char;
    let _ = i as char;
    let _ = c as bool;
    let _ = 65 as char;
}

fn operators(c: char) {
    let _ = c + 'a';
    let _ = c == 1;
    let _ = -c;
}

fn matches(c: char) -> i32 {
    match c {
        'a' => 1,
    }
}

fn ranges(c: char) -> i32 {
    match c {
        '\0'..='z' => 1,
        '\u{E000}'.. => 2,
    }
}

fn exhaustive(c: char) -> i32 {
    match c {
        '\0'..='\u{D7FF}' => 1,
        '\u{E000}'..='\u{10FFFF}' => 2,
    }
}

fn backwards(c: char) -> i32 {
    match c {
        'z'..='a' => 1,
        _ => 2,
    }
}

fn mismatch() {
    let x: char = 1;
    let y: u8 = 'a';
    let s: &[u8; 2] = b"abc";
}

fn main() {}
