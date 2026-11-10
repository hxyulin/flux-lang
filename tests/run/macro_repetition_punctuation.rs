macro_rules! target {
    ($t:ident: $bound:ident + => $name:ident) => {
        fn $name() -> i32 {
            17
        }
    };
}
macro_rules! forward { ($($t:ident: $($bound:ident)*),*) => { target!($($t: $($bound +)*)* => answer); }; }
fn main() {
    forward!(T: Clone);
    println!("{}", answer());
}
