// Constant evaluation reports the messages rustc can format at compile
// time: literals, `{}` of a `&str`, and `expect`'s.

const fn pick(o: Option<u8>) -> u8 {
    o.expect("needed a value")
}

const fn named(s: &str) -> usize {
    if s.len() > 3 {
        panic!("{}", s);
    }
    s.len()
}

const A: u8 = pick(None);
const B: usize = named("long name");
const C: () = panic!("{}", "direct");
const D: () = assert!(B == 0, "assertion message");

fn main() {
    let _ = (A, B, C, D);
}
