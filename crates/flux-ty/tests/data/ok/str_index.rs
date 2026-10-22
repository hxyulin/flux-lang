// Strings indexed by ranges of bytes, through `core`'s `Index` impls for
// `str`, and the methods that give parts of them.

fn main() {
    let s = "hello, world";
    let a = &s[..5];
    let b = &s[7..];
    let c = &s[1..=3];
    let d = &s[..];
    let (left, right) = s.split_at(5);
    let trimmed = "  x ".trim();
    let parts = s.split(", ").count();
    let by_char = s.split('o');
    let lines = "a\nb".lines();
    let words = s.split_whitespace();
    let mut buf = [0u8; 4];
    let e: &mut str = 'e'.encode_utf8(&mut buf);
}
