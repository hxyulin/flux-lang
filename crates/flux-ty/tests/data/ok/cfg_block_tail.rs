#![allow(unused_braces, unused_imports)]

fn choose() -> u32 {
    #[cfg(all())]
    { 7 }
    #[cfg(any())]
    { 9 }
}

fn forms(x: u32) -> u32 {
    #[cfg_attr(all(), cfg_attr(all(), cfg(all())))]
    if x == 0 { 10 } else { 11 }
    #[cfg_attr(all(), cfg_attr(all(), cfg(any())))]
    { missing() }
}

fn nested(x: u32) -> u32 {
    {
        #[cfg(all())]
        match x { 0 => 12, _ => return 13 }
        #[cfg(any())]
        let missing = missing();
        #[cfg(any())]
        use absent::Missing;
    }
    #[cfg(any())]
    99
}

fn explicit_semicolon() {
    { println!("semicolon"); };
    #[cfg(any())]
    { missing() }
}

fn empty() {
    #[cfg(any())]
    { missing() }
}

fn item_barrier() {
    { println!("item"); }
    const VALUE: u32 = 1;
    #[cfg(any())]
    { missing() }
}

macro_rules! select {
    ($value:expr) => {
        use core::option::Option as Imported;
        #[cfg(all())]
        { let value = $value; Imported::Some(value) }
        #[cfg(any())]
        { missing() }
    };
}

fn expanded(value: u32) -> Option<u32> {
    select! { value }
}

fn macro_predecessor() -> u32 {
    macro_rules! value { () => { 14 }; }
    value! {}
    #[cfg(any())]
    { missing() }
}

struct Trace(u32);
impl Drop for Trace {
    fn drop(&mut self) { println!("drop {}", self.0); }
}

fn drops() -> Trace {
    let _outer = Trace(1);
    { let _inner = Trace(2); Trace(3) }
    #[cfg(any())]
    { missing() }
}

fn main() {
    println!("{} {} {} {} {}", choose(), forms(0), forms(1), nested(0), nested(1));
    explicit_semicolon();
    empty();
    item_barrier();
    println!("{:?} {}", expanded(15), macro_predecessor());
    let value = drops();
    println!("value {}", value.0);
}
