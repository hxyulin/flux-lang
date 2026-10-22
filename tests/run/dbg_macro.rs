//! `dbg!` prints where it is, the expression and its value with
//! `{:#?}` to stderr, and gives the value back.

#[derive(Debug, Clone, Copy)]
struct P {
    x: i32,
}

fn twice(n: i32) -> i32 {
    dbg!(n * 2)
}

fn main() {
    let a = dbg!(twice(3) + 1);
    let p = dbg!(P { x: a });
    let (b, c) = dbg!(a, p.x + 1);
    dbg!();
    println!("{} {} {}", a, b, c);
}
