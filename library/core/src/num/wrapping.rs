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
        impl crate::ops::BitAnd for Wrapping<$ty> {type Output=Self;fn bitand(self,rhs:Self)->Self{Self(self.0 & rhs.0)}}
        impl crate::ops::BitOr for Wrapping<$ty> {type Output=Self;fn bitor(self,rhs:Self)->Self{Self(self.0 | rhs.0)}}
        impl crate::ops::BitXor for Wrapping<$ty> {type Output=Self;fn bitxor(self,rhs:Self)->Self{Self(self.0 ^ rhs.0)}}
        impl crate::ops::Shl<usize> for Wrapping<$ty> {type Output=Self;fn shl(self,rhs:usize)->Self{Self(self.0.wrapping_shl(rhs as u32))}}
        impl crate::ops::Shr<usize> for Wrapping<$ty> {type Output=Self;fn shr(self,rhs:usize)->Self{Self(self.0.wrapping_shr(rhs as u32))}}
        impl crate::ops::AddAssign for Wrapping<$ty> {fn add_assign(&mut self,rhs:Self){*self=*self + rhs;}}
        impl crate::ops::SubAssign for Wrapping<$ty> {fn sub_assign(&mut self,rhs:Self){*self=*self - rhs;}}
        impl crate::ops::MulAssign for Wrapping<$ty> {fn mul_assign(&mut self,rhs:Self){*self=*self * rhs;}}
        impl crate::ops::DivAssign for Wrapping<$ty> {fn div_assign(&mut self,rhs:Self){*self=*self / rhs;}}
        impl crate::ops::RemAssign for Wrapping<$ty> {fn rem_assign(&mut self,rhs:Self){*self=*self % rhs;}}
        impl crate::ops::BitAndAssign for Wrapping<$ty> {fn bitand_assign(&mut self,rhs:Self){*self=*self & rhs;}}
        impl crate::ops::BitOrAssign for Wrapping<$ty> {fn bitor_assign(&mut self,rhs:Self){*self=*self | rhs;}}
        impl crate::ops::BitXorAssign for Wrapping<$ty> {fn bitxor_assign(&mut self,rhs:Self){*self=*self ^ rhs;}}
        impl crate::ops::ShlAssign<usize> for Wrapping<$ty> {fn shl_assign(&mut self,rhs:usize){*self=*self << rhs;}}
        impl crate::ops::ShrAssign<usize> for Wrapping<$ty> {fn shr_assign(&mut self,rhs:usize){*self=*self >> rhs;}}
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
