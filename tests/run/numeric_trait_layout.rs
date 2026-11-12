use std::fmt::{self,Display};
struct Zero;
impl Display for Zero{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{(0.).fmt(f)}}
trait Pick{type Out;fn pick(self)->Self::Out;}
impl Pick for f32{type Out=f32;fn pick(self)->f32{self}}
impl Pick for f64{type Out=f64;fn pick(self)->f64{self}}
#[derive(Clone,Copy)]struct R<const N:u8>(u8);
const fn bound()->u8{23}
type Hours=R<{bound()}>;
#[repr(C)]#[derive(Clone,Copy)]struct Clock{hour:Hours,pad:[u8;7]}
impl Clock{const MIN:Self=Self{hour:R(0),pad:[0;7]};const fn bits(self)->u8{unsafe{std::mem::transmute::<Self,u64>(self) as u8}}}
type Seconds=R<{Clock::MIN.bits()}>;
struct Stamp{seconds:Seconds}
fn main(){
 let value=1.;let picked=value.pick();let _:f32=value;assert_eq!(picked,1f32);
 assert_eq!(format!("{}",Zero),"0");assert_eq!(Stamp{seconds:R(7)}.seconds.0,7);
 println!("{} {} {}",Zero,picked,std::mem::size_of::<Stamp>());
}
