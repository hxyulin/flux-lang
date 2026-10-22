//! `Vec::remove` past the end panics with rustc's message, at the call
//! (`#[track_caller]`), after printing what came before.

fn remove_at(v: &mut Vec<i32>, i: usize) -> i32 {
    v.remove(i)
}

fn main() {
    let mut v = vec![1, 2, 3];
    println!("{}", remove_at(&mut v, 1));
    println!("{v:?}");
    println!("{}", remove_at(&mut v, 5));
}
