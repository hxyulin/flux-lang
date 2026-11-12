use std::num::{NonZero,NonZeroU32};
const N:NonZero<u128>=NonZero::new(1u128<<100).unwrap();
const Z:Option<NonZero<u32>>=NonZero::new(0);
macro_rules! check{($t:ty,$alias:ty,$value:expr)=>{{
 let value=NonZero::<$t>::new($value).unwrap();let alias:$alias=value;let same:NonZero<$t>=alias;
 assert_eq!(same.get(),$value);assert_eq!(std::mem::size_of::<Option<NonZero<$t>>>(),std::mem::size_of::<$t>());
 assert!(NonZero::<$t>::new(0).is_none());println!("{} {:?}",same,same);
}}}
fn main(){let inferred=NonZero::new(42).unwrap();let _:NonZero<i32>=inferred;
 check!(u8,std::num::NonZeroU8,255);check!(u16,std::num::NonZeroU16,65535);check!(u32,NonZeroU32,42);check!(u64,std::num::NonZeroU64,1u64<<63);check!(u128,std::num::NonZeroU128,1u128<<100);check!(usize,std::num::NonZeroUsize,17);
 check!(i8,std::num::NonZeroI8,-128);check!(i16,std::num::NonZeroI16,-32768);check!(i32,std::num::NonZeroI32,-42);check!(i64,std::num::NonZeroI64,i64::MIN);check!(i128,std::num::NonZeroI128,i128::MIN);check!(isize,std::num::NonZeroIsize,-17);
 assert_eq!(N.get(),1u128<<100);assert!(Z.is_none());
 assert_eq!(unsafe{std::mem::transmute::<Option<NonZeroU32>,u32>(None)},0);
 assert_eq!(unsafe{std::mem::transmute::<Option<NonZeroU32>,u32>(NonZeroU32::new(42))},42);
}
