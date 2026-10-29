// This file has CRLF line endings: a backslash before CRLF continues a string.
fn main() {
    let s = "a\
        b";
    let b = b"c\
        d";
    let c = c"e\
        f";
    println!("{} {:?} {:?}", s, b, c);
    std::process::exit(s.len() as i32 + b.len() as i32);
}
