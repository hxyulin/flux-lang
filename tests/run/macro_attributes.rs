#![recursion_limit = "256"]
mod formatting {
    #![macro_use]
    macro_rules! make {
        ($name:ident) => {
            fn $name() -> u32 {
                73
            }
        };
    }
}
make!(answer);
macro_rules! sum {
    () => { 0u32 };
    ($head:expr $(,$tail:expr)*) => { $head + sum!($($tail),*) };
}
fn main() {
    let value = sum!(1, 2, 3, 4, 5);
    assert_eq!(value, 15);
    assert_eq!(answer(), 73);
    println!("macro attributes {} {}", answer(), value);
}
