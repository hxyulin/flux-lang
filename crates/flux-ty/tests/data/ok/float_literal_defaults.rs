// A float literal nothing else gives a type is `f64`, also when a trait
// bound on it is all that remains, and before closures are analyzed.

use std::fmt::Display;

fn show<T: Display>(t: T) -> T {
    t
}

fn main() {
    println!("{}", 1.5);
    let x = 2.5;
    println!("{x} {:?}", 0.25);
    let y = show(0.5);
    let pair = (1.0, 2);
    println!("{:?}", pair);
    let z = 4.0;
    let c = move || z * 2.0;
    println!("{} {}", c(), y);
}
