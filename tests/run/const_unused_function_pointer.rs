fn recur<T>() { recur::<Option<T>>(); }
const UNUSED: fn() = recur::<u8>;
fn main() { println!("unused constant remains unused"); }
