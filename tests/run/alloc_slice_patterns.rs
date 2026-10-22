//! Slice and array patterns on what `Vec` and `Box` hold: matching
//! `v[..]` and `as_slice()`, a `Box<[T]>`'s contents, and moving elements
//! out of a boxed array, which drops the rest and then frees the box.

fn sum(v: &[i32]) -> i32 {
    match v {
        [] => 0,
        [x, rest @ ..] => *x + sum(rest),
    }
}

fn describe(v: &Vec<String>) -> String {
    match v.as_slice() {
        [] => String::from("none"),
        [one] => format!("one: {}", one),
        [first, .., last] => format!("{} to {}", first, last),
    }
}

fn matches() {
    let v = vec![1, 2, 3, 4];
    println!("{}", sum(&v));
    if let [a, b, ..] = v[..] {
        println!("{} {}", a, b);
    }
    let names: Vec<String> = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    println!("{}", describe(&names));
    println!("{}", describe(&vec![String::from("x")]));
    println!("{}", describe(&Vec::new()));
    let b: Box<[i32]> = vec![7, 8, 9].into_boxed_slice();
    match *b {
        [x, .., z] => println!("{} {}", x, z),
        _ => println!("short"),
    }
    let arr = Box::new([String::from("p"), String::from("q")]);
    let [p, q] = *arr;
    println!("{}{}", p, q);
    let mut w = vec![1, 2, 3];
    if let [first, ..] = w.as_mut_slice() {
        *first = 10;
    }
    match &w[..] {
        [10, rest @ ..] => println!("rest {:?}", rest),
        _ => println!("no"),
    }
    let strs = vec![String::from("m"), String::from("n")];
    match &strs[..] {
        [s, t] => println!("{}{} {}", s, t, s.len() + t.len()),
        _ => {}
    }
}

struct D(&'static str);
impl Drop for D {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn moves() {
    let arr = Box::new([D("a"), D("b"), D("c")]);
    let [_, b, ..] = *arr;
    println!("took {}", b.0);
    let v = vec![[D("x"), D("y")], [D("z"), D("w")]];
    for pair in v {
        let [first, _] = pair;
        println!("first {}", first.0);
    }
    let boxed = Box::new([String::from("s1"), String::from("s2"), String::from("s3")]);
    let [.., last] = *boxed;
    println!("{}", last);
}

fn main() {
    matches();
    moves();
}
