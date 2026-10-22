//! The comparison traits' methods called on references to unsized
//! values and with arguments that take a reference off (`a.partial_cmp(&b)`
//! for `&str`s, `x.cmp(&y)` for `&[T]`s), as rustc's method probe finds
//! `str`'s and `[T]`'s impls and then coerces the arguments.

use core::cmp::Ordering;

fn order(o: Option<Ordering>) -> i32 {
    match o {
        Some(Ordering::Less) => -1,
        Some(Ordering::Equal) => 0,
        Some(Ordering::Greater) => 1,
        None => 9,
    }
}

fn by_name<'a>(names: &mut [&'a str]) {
    names.sort_by(|a, b| a.partial_cmp(b).unwrap());
}

fn main() {
    let a = "abc";
    let b = "abd";
    let x: &[i32] = &[1, 2];
    let y: &[i32] = &[1, 3];
    let mut out = 0;
    out = out * 3 + order(a.partial_cmp(b)) + 1;
    out = out * 3 + order(a.partial_cmp(&b)) + 1;
    out = out * 3 + order(Some(a.cmp(b))) + 1;
    out = out * 3 + order(Some(b.cmp(&a))) + 1;
    out = out * 3 + order(x.partial_cmp(y)) + 1;
    out = out * 3 + order(x.partial_cmp(&y)) + 1;
    out = out * 3 + order(Some(y.cmp(&x))) + 1;
    out = out * 3 + order(Some("x".cmp("x"))) + 1;
    println!("{out}");
    println!("{} {} {}", a.lt(&b), a.eq(b), x.ge(&y));
    println!("{} {}", a.max(b), x.min(y).len());
    let mut names = ["pear", "apple", "fig"];
    by_name(&mut names);
    println!("{:?}", names);
    let p = 3i32;
    let q = 5i32;
    println!("{:?} {:?}", (&p).partial_cmp(&&q), p.cmp(&q));
}
