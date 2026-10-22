// Format strings checked as rustc checks them, each error where rustc
// points: inside the string where it can, escapes included.

fn main() {
    let x = 1;
    println!("a {0} {2}", x, x);
    println!("b {x:?} {:>width$}", 1);
    println!("tab\t{}");
    println!("{:?}", x, 2);
    println!("{} {}", x);
    println!("{:y}", 1);
    println!("{:70000}", 1);
    println!("{:.*}", 1);
    println!("{a}", b = 2);
    println!("{}", x, y = 3, z = 4);
    println!(r#"raw "{}" {2}"#, 1);
    println!("unicode é {} {3}", 1);
}
