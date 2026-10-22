const fn check(n: u32) -> u32 {
    assert!(n < 10, "too big");
    if n == 7 {
        panic!("seven");
    }
    if n == 8 {
        unreachable!();
    }
    n
}

const A: u32 = check(12);
const B: u32 = check(7);
const C: u32 = check(8);
const D: () = assert!(A > 2);

fn main() {
    let _ = (A, B, C, D);
}
