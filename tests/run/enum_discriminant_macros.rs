#[macro_use]mod definitions{
    macro_rules! bitcast{($value:expr)=>{{$value as _}}}
}
#[repr(u32)]enum Flags{One=bitcast!(1),Two=bitcast!(2)}
macro_rules! value{()=>{7}}
#[repr(u32)]enum Earlier{Item=value!()}
macro_rules! value{()=>{11}}
#[repr(u32)]enum Later{Item=value!()}
#[repr(u32)]enum Referenced{Base=13,Next=bitcast!(Self::Base as u32+1)}
fn main(){
    assert_eq!(Flags::One as u32,1);assert_eq!(Flags::Two as u32,2);
    assert_eq!(Earlier::Item as u32,7);assert_eq!(Later::Item as u32,11);
    assert_eq!(Referenced::Next as u32,14);
    println!("{} {} {}",Earlier::Item as u32,Later::Item as u32,Referenced::Next as u32);
}
