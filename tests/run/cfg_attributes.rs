#![allow(dead_code, unused_macros, unexpected_cfgs)]
#![cfg_attr(all(), allow(unused_variables))]

#[cfg_attr(all(), cfg(any()))]
mod absent_file;
#[cfg_attr(all(), cfg_attr(not(test), cfg(any())))]
fn value() -> i32 { missing_type!() }
#[cfg_attr(any(), unknown_attribute)]
#[cfg_attr(all(), inline, cold)]
fn value() -> i32 { 7 }

#[cfg_attr(all(), derive(Clone, Copy))]
#[cfg_attr(all(), repr(C))]
struct Value { value: i32 }

#[cfg_attr(all(), macro_use)]
mod macros {
    macro_rules! choose { () => { 11 }; }
}
// The shape of memchr's disabled logging macros: the unavailable dependency
// must not be resolved after the nested macros expand in statement position.
macro_rules! log { ($($tt:tt)*) => { #[cfg(feature = "logging")] { $($tt)* } }; }
macro_rules! trace { ($($tt:tt)*) => { log!(missing_log::trace!($($tt)*)) }; }
macro_rules! generated { () => {
    #[cfg_attr(all(), cfg(any()))]
    fn generated() -> missing_type!() { missing_macro!() }
    #[cfg_attr(all(), inline)]
    fn generated() -> i32 { 41 }
}; }
generated!();
macro_rules! configured_statement { () => { #[cfg_attr(all(), cfg(all()))] { println!("{}", generated()); } }; }
mod nested {
    pub fn value() -> i32 { choose!() }
}
mod hidden {
    #![cfg_attr(all(), cfg(any()))]
    use missing_crate::Never;
    compile_error!("configured out");
}
fn hidden_fn() { #![cfg(any())] missing_macro!(); }

fn sum(a: i32, b: i32) -> i32 { a + b }
#[cfg_attr(all(), repr(align(64)))]
struct Aligned(u8);
fn main() {
    let aligned = Aligned(7);
    println!("{} {} {}", core::mem::align_of::<Aligned>(), core::mem::size_of::<Aligned>(), aligned.0);
    trace!("disabled logging");
    configured_statement!();
    #[cfg_attr(all(), cfg(any()))]
    let bad: missing_type!() = missing_macro!();
    #[cfg(any())]
    missing_macro!();
    #[cfg(any())]
    { fn broken() -> missing_type!() { missing_macro!() } }
    #[cfg(any())]
    macro_rules! choose { () => { missing_macro!() }; }
    let v = Value { value: value() };
    let copy = v;
    println!("{} {} {} {}", v.value, copy.value, choose!(), nested::value());
    let a = [#[cfg(any())] missing_macro!(), 13, 17];
    let t = (#[cfg(any())] missing_macro!(), 19, 23);
    println!("{} {} {}", a[0] + a[1], t.0 + t.1,
        sum(#[cfg(any())] missing_macro!(), 29, 31));
    let selected = match 1 { #[cfg(any())] _ => missing_macro!(), _ => 37 };
    let record = Value { #[cfg(any())] absent: missing_macro!(), value: selected };
    println!("{}", record.value);
    #[cfg(any())]
    missing_macro!()
}
