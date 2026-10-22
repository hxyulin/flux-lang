// A `str` is indexed by ranges of `usize` only.

fn main() {
    let s = "hello";
    let a = &s[1i32..3];
    let b = s[0];
}
