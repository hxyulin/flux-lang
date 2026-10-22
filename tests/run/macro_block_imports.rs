mod first {
    pub fn value(x: i32) -> i32 { x + 10 }
    pub struct One(pub i32);
    impl One { pub fn value(&self) -> i32 { self.0 + 20 } }
}
mod second {
    pub fn value(x: i32) -> i32 { x + 30 }
}
fn value(x: i32) -> i32 { x + 100 }
macro_rules! imports {
    () => { use crate::first::{value as chosen, One}; };
}
macro_rules! nested {
    ($x:expr) => {{
        imports!();
        chosen($x) + One($x).value()
    }};
}
macro_rules! selected {
    ($x:expr) => {{
        #[cfg(any())] { use crate::missing::value; value($x) }
        #[cfg(all())] {
            use crate::second::value;
            value($x)
        }
    }};
}
fn tail_import() {
    println!("tail={}", chosen(11));
    imports!{}
}
fn shadow(x: i32) {
    let chosen = x;
    macro_rules! captured { () => { chosen }; }
    {
        imports!();
        println!("item-shadow={} capture={}", chosen(x), captured!()(x));
        let chosen = x + 1;
        println!("local-shadow={}", chosen);
    }
    println!("outer-restored={}", chosen);
}
fn main() {
    shadow(13);
    tail_import();
    {
        #[cfg(all())]
        imports!();
        println!("attributed={}", chosen(12));
    }
    // Imports are block-wide, including before their macro invocation.
    {
        println!("before={}", chosen(1));
        imports!();
        println!("after={}", One(2).value());
    }
    println!("nested={} selected={}", nested!(3), selected!(4));
    {
        use crate::second::value;
        println!("shadow={} inner={} restored={}", value(5), selected!(6), value(7));
    }
    // A definition-site local remains visible across retained expansions.
    let x = 8;
    macro_rules! local { () => {{ use crate::first::value; value(x) }}; }
    let x = 9;
    println!("hygiene={} caller={} sibling={}", local!(), x, value(10));
}
