use std::num::NonZero;
macro_rules! check{
 ($source:ty => $($target:ty),+) => {$(
  {
   let small=NonZero::<$source>::new(<$source>::MIN).unwrap_or_else(||NonZero::<$source>::new(1).unwrap());
   let converted:NonZero<$target>=small.into();assert_eq!(converted.get(),<$target>::from(small.get()));
   let big=NonZero::<$source>::new(<$source>::MAX).unwrap();
   assert_eq!(NonZero::<$target>::from(big).get(),<$target>::from(big.get()));
  }
 )+};
}
fn main(){
 check!(u8=>u16,u32,u64,u128,usize);check!(u16=>u32,u64,u128,usize);check!(u32=>u64,u128);check!(u64=>u128);
 check!(i8=>i16,i32,i64,i128,isize);check!(i16=>i32,i64,i128,isize);check!(i32=>i64,i128);check!(i64=>i128);
 check!(u8=>i16,i32,i64,i128,isize);check!(u16=>i32,i64,i128);check!(u32=>i64,i128);check!(u64=>i128);
 let length=std::num::NonZeroUsize::from(std::num::NonZeroU8::new(21).unwrap());assert_eq!(length.get(),21);
 println!("nonzero widening {}",length.get());
}
