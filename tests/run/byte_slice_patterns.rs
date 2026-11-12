const EMPTY: &[u8] = b"";
const ALPHA: &[u8] = b"alpha";
const BINARY: &[u8] = &[0, 255];
fn classify(bytes: &[u8]) -> u8 {
    match bytes { EMPTY => 0, ALPHA => 1, BINARY => 2, _ => 3 }
}
fn exhaustive(bytes: &[u8]) -> bool {
    match bytes { EMPTY => false, [_, ..] => true }
}
fn main() {
    for bytes in [b"".as_slice(), b"alpha", &[0, 255], b"alph", b"alphab"] {
        println!("{} {}", classify(bytes), exhaustive(bytes));
    }
}
