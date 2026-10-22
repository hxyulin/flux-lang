//! Slicing a string inside a character panics with rustc's message,
//! which names the character and its bytes.

fn cut(s: &str, end: usize) -> &str {
    &s[..end]
}

fn main() {
    println!("{}", cut("añb", 1));
    println!("{}", cut("añb", 3));
    println!("{}", cut("añb", 2));
}
