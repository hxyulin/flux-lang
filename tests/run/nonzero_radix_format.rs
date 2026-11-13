use std::fmt;
use std::num::{NonZeroU32, NonZeroI32};
struct Id(NonZeroU32);
impl fmt::LowerHex for Id {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { self.0.fmt(f) }
}
fn main() {
    let value=NonZeroU32::new(31).unwrap();
    println!("{:x} {:X} {:o} {:b}", Id(value), value, value, value);
    println!("{:#010x} {:#012b}", value,value);
    println!("{:x}", NonZeroI32::new(-1).unwrap());
}
