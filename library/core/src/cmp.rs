//! Comparison: `a == b` and `a != b` on types other than integers and
//! `bool` are `PartialEq::eq(&a, &b)` and `PartialEq::ne(&a, &b)`.

/// The `==` and `!=` operators.
#[lang = "eq"]
#[diagnostic::on_unimplemented(message = "can't compare `{Self}` with `{Rhs}`")]
pub trait PartialEq<Rhs = Self> {
    fn eq(&self, other: &Rhs) -> bool;
    fn ne(&self, other: &Rhs) -> bool {
        !self.eq(other)
    }
}

/// Equality that is reflexive: `a == a` for every `a`.
pub trait Eq: PartialEq {}

impl PartialEq for i8 { fn eq(&self, other: &i8) -> bool { *self == *other } fn ne(&self, other: &i8) -> bool { *self != *other } }
impl Eq for i8 {}
impl PartialEq for i16 { fn eq(&self, other: &i16) -> bool { *self == *other } fn ne(&self, other: &i16) -> bool { *self != *other } }
impl Eq for i16 {}
impl PartialEq for i32 { fn eq(&self, other: &i32) -> bool { *self == *other } fn ne(&self, other: &i32) -> bool { *self != *other } }
impl Eq for i32 {}
impl PartialEq for i64 { fn eq(&self, other: &i64) -> bool { *self == *other } fn ne(&self, other: &i64) -> bool { *self != *other } }
impl Eq for i64 {}
impl PartialEq for i128 { fn eq(&self, other: &i128) -> bool { *self == *other } fn ne(&self, other: &i128) -> bool { *self != *other } }
impl Eq for i128 {}
impl PartialEq for isize { fn eq(&self, other: &isize) -> bool { *self == *other } fn ne(&self, other: &isize) -> bool { *self != *other } }
impl Eq for isize {}
impl PartialEq for u8 { fn eq(&self, other: &u8) -> bool { *self == *other } fn ne(&self, other: &u8) -> bool { *self != *other } }
impl Eq for u8 {}
impl PartialEq for u16 { fn eq(&self, other: &u16) -> bool { *self == *other } fn ne(&self, other: &u16) -> bool { *self != *other } }
impl Eq for u16 {}
impl PartialEq for u32 { fn eq(&self, other: &u32) -> bool { *self == *other } fn ne(&self, other: &u32) -> bool { *self != *other } }
impl Eq for u32 {}
impl PartialEq for u64 { fn eq(&self, other: &u64) -> bool { *self == *other } fn ne(&self, other: &u64) -> bool { *self != *other } }
impl Eq for u64 {}
impl PartialEq for u128 { fn eq(&self, other: &u128) -> bool { *self == *other } fn ne(&self, other: &u128) -> bool { *self != *other } }
impl Eq for u128 {}
impl PartialEq for usize { fn eq(&self, other: &usize) -> bool { *self == *other } fn ne(&self, other: &usize) -> bool { *self != *other } }
impl Eq for usize {}
impl PartialEq for bool { fn eq(&self, other: &bool) -> bool { *self == *other } fn ne(&self, other: &bool) -> bool { *self != *other } }
impl Eq for bool {}
impl PartialEq for () { fn eq(&self, _other: &()) -> bool { true } }
impl Eq for () {}
