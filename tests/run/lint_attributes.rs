#![allow(overflowing_literals)]
const MASK:i32=0xff000000;
#[repr(i8)] enum Signed {A=128}
#[repr(u8)] enum Unsigned {A=257}
fn value<const N:i8>()->i8{N}
#[warn(overflowing_literals)] fn warned()->i8{255}
macro_rules! local {()=>{{#[allow(overflowing_literals)] let value:i8=255;value}}}
fn main(){
    assert_eq!(MASK,-16777216);
    assert_eq!(Signed::A as i8,-128);assert_eq!(Unsigned::A as u8,1);
    assert_eq!(value::<255>(),-1);assert_eq!(value::<-129>(),127);
    assert_eq!(warned(),-1);assert_eq!(local!(),-1);
    let x:i8=255;assert!(matches!(x,255));
    assert!(1e1000f64.is_infinite());
    println!("literal scopes and wrapping passed");
}
