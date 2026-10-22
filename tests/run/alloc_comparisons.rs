//! `alloc`'s comparisons: `String` with `str` and `&str` either way
//! round, and `Vec` with `Vec`s, slices, arrays and references to them.

fn main() {
    let s = String::from("abc");
    let t: &str = "abc";
    println!("{} {} {} {}", s == t, t == s, s == *t, *t == s);
    println!("{} {}", s != "abd", "abd" != s);
    let v = vec![1, 2, 3];
    let mut a = [1, 2, 3];
    println!("{} {} {}", v == [1, 2, 3], v == &[1, 2, 3], v == a[..]);
    println!("{} {} {}", a[..] == v, &a[..] == v, v == &a[..]);
    println!("{} {}", v == &mut a[..], &mut a[..] == v);
    let w: Vec<i64> = vec![1, 2];
    println!("{} {}", w == vec![1, 2], w != [1, 3]);
    let words = vec![String::from("x"), String::from("y")];
    println!("{}", words == ["x", "y"]);
    println!("{}", words[..] == ["x", "y"]);
}
