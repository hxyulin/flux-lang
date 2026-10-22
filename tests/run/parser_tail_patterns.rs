// Range-to patterns must stay parenthesized patterns, and attributed
// macro expansions must preserve their tail value, including nested calls.
macro_rules! classify {
    ($x:expr) => {
        #[allow(unused)]
        match $x { (..=5) => 1, (..10) => 2, _ => 3 }
    };
}
macro_rules! nested {
    ($x:expr) => { classify!($x) };
}
fn tail(x: u8) -> bool {
    matches!(x, (..=5))
}
fn branch(x: u8) -> bool {
    if x < 10 { matches!(x, 1 | 3) } else { matches!(x, (..=15)) }
}
fn attributed(x: u8) -> u8 {
    #[allow(unused)]
    match x { (..=5) => 1, _ => 2 }
}
fn main() {
    for x in [0, 1, 3, 5, 6, 9, 10, 15, 16, 255] {
        let closure = || { matches!(x, (..=5)) };
        let statement = { matches!(x, 1 | 3); 7 };
        let classification = { nested!(x) };
        println!("{x}: {} {} {} {} {} {}", tail(x), branch(x), closure(),
            classification, attributed(x), statement);
        let pair = (x, 12);
        println!("tuple {}", match pair { (..=5, _) => 1, (..10, _) => 2, (..) => 3 });
        let (..=5,) = (x,) else { println!("else"); continue };
        println!("small");
    }
}
