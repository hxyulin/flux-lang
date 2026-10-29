// Float literals with no annotation default to `f64`, including where
// only a trait bound (formatting, a generic call) constrains them.

use std::fmt::Display;

fn show<T: Display>(t: T) -> T {
    println!("show {t}");
    t
}

fn main() {
    println!("{}", 1.5);
    println!("{:?} {}", 0.1 + 0.2, 1.0 / 3.0);
    let x = 2.5;
    let y = show(x * 4.0);
    let pair = (1.0, 2);
    println!("{pair:?}");
    let z = 0.75;
    let scale = move |v| v * z;
    let w = scale(8.0);
    println!("{} {}", w, y - w);
    let mut total = 0.0;
    for v in [0.5, 1.5, 2.25] {
        total += v;
    }
    println!("{total}");
    std::process::exit((w + y) as i32);
}
