fn keep(x: &i32) -> Option<&i32> { Some(x) }
fn main() { let n = 42; println!("{}", *[&n].into_iter().find_map(keep).unwrap()); }
