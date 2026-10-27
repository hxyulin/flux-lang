//! Integer arithmetic with modular overflow.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Wrapping<T>(pub T);
impl<T:crate::fmt::Debug> crate::fmt::Debug for Wrapping<T> {fn fmt(&self,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result{self.0.fmt(f)}}
impl<T:crate::fmt::Display> crate::fmt::Display for Wrapping<T> {fn fmt(&self,f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result{self.0.fmt(f)}}
macro_rules! wrapping_integer {
    ($ty:ty) => {
        impl crate::ops::Add for Wrapping<$ty> {type Output=Self;fn add(self,rhs:Self)->Self{Self(self.0.wrapping_add(rhs.0))}}
        impl crate::ops::Sub for Wrapping<$ty> {type Output=Self;fn sub(self,rhs:Self)->Self{Self(self.0.wrapping_sub(rhs.0))}}
        impl crate::ops::Mul for Wrapping<$ty> {type Output=Self;fn mul(self,rhs:Self)->Self{Self(self.0.wrapping_mul(rhs.0))}}
        impl crate::ops::Div for Wrapping<$ty> {type Output=Self;fn div(self,rhs:Self)->Self{Self(self.0.wrapping_div(rhs.0))}}
        impl crate::ops::Rem for Wrapping<$ty> {type Output=Self;fn rem(self,rhs:Self)->Self{Self(self.0.wrapping_rem(rhs.0))}}
        impl crate::ops::Not for Wrapping<$ty> {type Output=Self;fn not(self)->Self{Self(!self.0)}}
        impl crate::ops::Neg for Wrapping<$ty> {type Output=Self;fn neg(self)->Self{Self(self.0.wrapping_neg())}}
    };
}
wrapping_integer!(i8);
wrapping_integer!(i16);
wrapping_integer!(i32);
wrapping_integer!(i64);
wrapping_integer!(i128);
wrapping_integer!(isize);
wrapping_integer!(u8);
wrapping_integer!(u16);
wrapping_integer!(u32);
wrapping_integer!(u64);
wrapping_integer!(u128);
wrapping_integer!(usize);
