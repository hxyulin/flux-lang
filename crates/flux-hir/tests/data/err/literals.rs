fn chars() {
    let b = '\x80';
    let c = '\q';
    let d = '\u{d800}';
    let e = '\u{110000}';
    let f = '\u{}';
    let g = '\u{1234567}';
    let h = '\u41';
    let i = '	';
}

fn bytes() {
    let a = b'é';
    let b = b'\u{41}';
    let c = b'\q';
    let d = b'\x8g';
    let e = b"é";
    let f = br"é";
    let g = b"\u{41}";
}

fn strings() {
    let a = "\x80";
    let b = "\q";
    let c = "\u{_1}";
}
