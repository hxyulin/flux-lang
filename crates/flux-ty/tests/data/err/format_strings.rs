// What format strings ask of their arguments' types: counts are
// `usize`s, and each value implements its placeholder's trait.

struct S;

fn main() {
    let w = 5i32;
    println!("{:1$}", 1, w);
    println!("{:.*}", 2u8, 1);
    println!("{:p}", 1);
    println!("{:x} {:e}", S, S);
    println!("{:?}", S);
}
