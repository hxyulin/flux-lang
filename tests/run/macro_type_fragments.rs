macro_rules! shift { ($t:ty, $n:expr) => { 1 as $t << $n } }
macro_rules! inner {
    ((u32)) => { false };
    ($t:ty) => { true };
}
macro_rules! forwarded { ($t:ty) => { inner!($t) } }
macro_rules! name { ($t:ty) => { stringify!($t) } }
mod imported {
    pub struct Word(pub u32);
}

macro_rules! read_import {
    ($ty:ty) => {{
        use {$ty as Word};
        Word(21).0 * 2
    }};
}

macro_rules! forward_import {
    ($ty:ty) => { read_import!($ty) };
}

mod scoped {
    macro_rules! word { () => { u32 } }
    pub(crate) use word;
}

fn identity<T>(value: T) -> T { value }

fn scoped_types() -> u32 {
    {
        use scoped::word;
        let first: word!() = identity::<word!()>(20);
        struct Local;
        impl Local {
            fn value() -> u32 {
                let second: word!() = 22;
                second
            }
        }
        first + Local::value()
    }
}

fn main() {
    assert_eq!(shift!(u32, 5), 32);
    assert!(forwarded!(u32));
    assert!(!inner!((u32)));
    assert_eq!(name!(u32), "u32");
    assert_eq!(forward_import!(imported::Word), 42);
    println!("{} {} {}", shift!(u32, 5), name!(u32), scoped_types());
}
