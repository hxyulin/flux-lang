//! Integer arithmetic that clamps overflow to the representable range.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Saturating<T>(pub T);
impl<T: crate::fmt::Debug> crate::fmt::Debug for Saturating<T> {
    fn fmt(&self, f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result {self.0.fmt(f)}
}
impl<T: crate::fmt::Display> crate::fmt::Display for Saturating<T> {
    fn fmt(&self, f:&mut crate::fmt::Formatter<'_>)->crate::fmt::Result {self.0.fmt(f)}
}
macro_rules! saturating_integer {
    ($ty:ty) => {
        impl Saturating<$ty> {
            pub const MIN:Self=Self(<$ty>::MIN);
            pub const MAX:Self=Self(<$ty>::MAX);
            pub const fn pow(self,exp:u32)->Self {Self(self.0.saturating_pow(exp))}
        }
        impl crate::ops::Add for Saturating<$ty> {
            type Output=Self;
            fn add(self,rhs:Self)->Self {Self(self.0.saturating_add(rhs.0))}
        }
        impl crate::ops::AddAssign for Saturating<$ty> {
            fn add_assign(&mut self,rhs:Self) {*self=*self + rhs;}
        }
        impl crate::ops::Sub for Saturating<$ty> {
            type Output=Self;
            fn sub(self,rhs:Self)->Self {Self(self.0.saturating_sub(rhs.0))}
        }
        impl crate::ops::SubAssign for Saturating<$ty> {
            fn sub_assign(&mut self,rhs:Self) {*self=*self - rhs;}
        }
        impl crate::ops::Mul for Saturating<$ty> {
            type Output=Self;
            fn mul(self,rhs:Self)->Self {Self(self.0.saturating_mul(rhs.0))}
        }
        impl crate::ops::MulAssign for Saturating<$ty> {
            fn mul_assign(&mut self,rhs:Self) {*self=*self * rhs;}
        }
        impl crate::ops::Div for Saturating<$ty> {
            type Output=Self;
            fn div(self,rhs:Self)->Self {Self(self.0.saturating_div(rhs.0))}
        }
        impl crate::ops::DivAssign for Saturating<$ty> {
            fn div_assign(&mut self,rhs:Self) {*self=*self / rhs;}
        }
        impl crate::ops::Rem for Saturating<$ty> {
            type Output=Self;
            fn rem(self,rhs:Self)->Self {Self(self.0.checked_rem(rhs.0).expect("attempt to calculate the remainder with overflow or a divisor of zero"))}
        }
        impl crate::ops::RemAssign for Saturating<$ty> {
            fn rem_assign(&mut self,rhs:Self) {*self=*self % rhs;}
        }
    };
}
saturating_integer!(u8);
saturating_integer!(u16);
saturating_integer!(u32);
saturating_integer!(u64);
saturating_integer!(u128);
saturating_integer!(usize);
saturating_integer!(i8);
saturating_integer!(i16);
saturating_integer!(i32);
saturating_integer!(i64);
saturating_integer!(i128);
saturating_integer!(isize);
