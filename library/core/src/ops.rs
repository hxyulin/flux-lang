//! Overloadable operators. On integers and `bool` the operators are
//! built in; on other types, `a + b` is `Add::add(a, b)`, `-a` is
//! `Neg::neg(a)`, `a += b` is `AddAssign::add_assign(&mut a, b)`, and so
//! on, through these traits, which the compiler knows by their `lang`
//! names. The integer types and `bool` implement them too, so that
//! generic code can use them; the impls are written out for each type,
//! since flux has no macros. Operators on references to them (`&a + b`,
//! `-&a`) are calls of these traits' methods, as in rustc: the impls for
//! references are at the end.
//!
//! `Drop` is here too, as in Rust: the code that runs when a value goes
//! out of scope, and so are the `Fn`-family traits, which closures
//! implement.

mod function;
mod index;
mod range;
mod try_trait;

pub use function::{Fn, FnMut, FnOnce};
pub use index::{Index, IndexMut};
pub use range::{
    Bound, Range, RangeBounds, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive,
};
pub use try_trait::ControlFlow;
// Unstable in rustc: `core` uses them, programs cannot name them.
pub(crate) use try_trait::{FromResidual, Try};

/// The `+` operator: addition.
#[lang = "add"]
#[diagnostic::on_unimplemented(message = "cannot add `{Rhs}` to `{Self}`")]
pub trait Add<Rhs = Self> {
    type Output;
    fn add(self, rhs: Rhs) -> Self::Output;
}

/// The `+=` operator.
#[lang = "add_assign"]
#[diagnostic::on_unimplemented(message = "cannot add-assign `{Rhs}` to `{Self}`")]
pub trait AddAssign<Rhs = Self> {
    fn add_assign(&mut self, rhs: Rhs);
}

/// The `-` operator: subtraction.
#[lang = "sub"]
#[diagnostic::on_unimplemented(message = "cannot subtract `{Rhs}` from `{Self}`")]
pub trait Sub<Rhs = Self> {
    type Output;
    fn sub(self, rhs: Rhs) -> Self::Output;
}

/// The `-=` operator.
#[lang = "sub_assign"]
#[diagnostic::on_unimplemented(message = "cannot subtract-assign `{Rhs}` from `{Self}`")]
pub trait SubAssign<Rhs = Self> {
    fn sub_assign(&mut self, rhs: Rhs);
}

/// The `*` operator: multiplication.
#[lang = "mul"]
#[diagnostic::on_unimplemented(message = "cannot multiply `{Self}` by `{Rhs}`")]
pub trait Mul<Rhs = Self> {
    type Output;
    fn mul(self, rhs: Rhs) -> Self::Output;
}

/// The `*=` operator.
#[lang = "mul_assign"]
#[diagnostic::on_unimplemented(message = "cannot multiply-assign `{Self}` by `{Rhs}`")]
pub trait MulAssign<Rhs = Self> {
    fn mul_assign(&mut self, rhs: Rhs);
}

/// The `/` operator: division.
#[lang = "div"]
#[diagnostic::on_unimplemented(message = "cannot divide `{Self}` by `{Rhs}`")]
pub trait Div<Rhs = Self> {
    type Output;
    fn div(self, rhs: Rhs) -> Self::Output;
}

/// The `/=` operator.
#[lang = "div_assign"]
#[diagnostic::on_unimplemented(message = "cannot divide-assign `{Self}` by `{Rhs}`")]
pub trait DivAssign<Rhs = Self> {
    fn div_assign(&mut self, rhs: Rhs);
}

/// The `%` operator: remainder.
#[lang = "rem"]
#[diagnostic::on_unimplemented(message = "cannot calculate the remainder of `{Self}` divided by `{Rhs}`")]
pub trait Rem<Rhs = Self> {
    type Output;
    fn rem(self, rhs: Rhs) -> Self::Output;
}

/// The `%=` operator.
#[lang = "rem_assign"]
#[diagnostic::on_unimplemented(message = "cannot calculate and assign the remainder of `{Self}` divided by `{Rhs}`")]
pub trait RemAssign<Rhs = Self> {
    fn rem_assign(&mut self, rhs: Rhs);
}

/// The `&` operator: bitwise AND.
#[lang = "bitand"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} & {Rhs}`")]
pub trait BitAnd<Rhs = Self> {
    type Output;
    fn bitand(self, rhs: Rhs) -> Self::Output;
}

/// The `&=` operator.
#[lang = "bitand_assign"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} &= {Rhs}`")]
pub trait BitAndAssign<Rhs = Self> {
    fn bitand_assign(&mut self, rhs: Rhs);
}

/// The `|` operator: bitwise OR.
#[lang = "bitor"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} | {Rhs}`")]
pub trait BitOr<Rhs = Self> {
    type Output;
    fn bitor(self, rhs: Rhs) -> Self::Output;
}

/// The `|=` operator.
#[lang = "bitor_assign"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} |= {Rhs}`")]
pub trait BitOrAssign<Rhs = Self> {
    fn bitor_assign(&mut self, rhs: Rhs);
}

/// The `^` operator: bitwise XOR.
#[lang = "bitxor"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} ^ {Rhs}`")]
pub trait BitXor<Rhs = Self> {
    type Output;
    fn bitxor(self, rhs: Rhs) -> Self::Output;
}

/// The `^=` operator.
#[lang = "bitxor_assign"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} ^= {Rhs}`")]
pub trait BitXorAssign<Rhs = Self> {
    fn bitxor_assign(&mut self, rhs: Rhs);
}

/// The `<<` operator: left shift.
#[lang = "shl"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} << {Rhs}`")]
pub trait Shl<Rhs = Self> {
    type Output;
    fn shl(self, rhs: Rhs) -> Self::Output;
}

/// The `<<=` operator.
#[lang = "shl_assign"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} <<= {Rhs}`")]
pub trait ShlAssign<Rhs = Self> {
    fn shl_assign(&mut self, rhs: Rhs);
}

/// The `>>` operator: right shift.
#[lang = "shr"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} >> {Rhs}`")]
pub trait Shr<Rhs = Self> {
    type Output;
    fn shr(self, rhs: Rhs) -> Self::Output;
}

/// The `>>=` operator.
#[lang = "shr_assign"]
#[diagnostic::on_unimplemented(message = "no implementation for `{Self} >>= {Rhs}`")]
pub trait ShrAssign<Rhs = Self> {
    fn shr_assign(&mut self, rhs: Rhs);
}

/// Code run when a value is dropped: when its binding goes out of
/// scope, when a temporary's statement ends, or when it is overwritten.
/// It runs before the value's fields are dropped, and may not be called
/// directly; `drop(x)` drops `x` early.
#[lang = "drop"]
pub trait Drop {
    fn drop(&mut self);
}

/// The unary `-` operator.
#[lang = "neg"]
pub trait Neg {
    type Output;
    fn neg(self) -> Self::Output;
}

/// The unary `!` operator.
#[lang = "not"]
pub trait Not {
    type Output;
    fn not(self) -> Self::Output;
}

impl Add for i8 { type Output = i8; #[rustc_inherit_overflow_checks] fn add(self, rhs: i8) -> i8 { self + rhs } }
impl AddAssign for i8 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: i8) { *self += rhs; } }
impl Sub for i8 { type Output = i8; #[rustc_inherit_overflow_checks] fn sub(self, rhs: i8) -> i8 { self - rhs } }
impl SubAssign for i8 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: i8) { *self -= rhs; } }
impl Mul for i8 { type Output = i8; #[rustc_inherit_overflow_checks] fn mul(self, rhs: i8) -> i8 { self * rhs } }
impl MulAssign for i8 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: i8) { *self *= rhs; } }
impl Div for i8 { type Output = i8; #[rustc_inherit_overflow_checks] fn div(self, rhs: i8) -> i8 { self / rhs } }
impl DivAssign for i8 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: i8) { *self /= rhs; } }
impl Rem for i8 { type Output = i8; #[rustc_inherit_overflow_checks] fn rem(self, rhs: i8) -> i8 { self % rhs } }
impl RemAssign for i8 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: i8) { *self %= rhs; } }
impl BitAnd for i8 { type Output = i8; fn bitand(self, rhs: i8) -> i8 { self & rhs } }
impl BitAndAssign for i8 { fn bitand_assign(&mut self, rhs: i8) { *self &= rhs; } }
impl BitOr for i8 { type Output = i8; fn bitor(self, rhs: i8) -> i8 { self | rhs } }
impl BitOrAssign for i8 { fn bitor_assign(&mut self, rhs: i8) { *self |= rhs; } }
impl BitXor for i8 { type Output = i8; fn bitxor(self, rhs: i8) -> i8 { self ^ rhs } }
impl BitXorAssign for i8 { fn bitxor_assign(&mut self, rhs: i8) { *self ^= rhs; } }
impl Neg for i8 { type Output = i8; #[rustc_inherit_overflow_checks] fn neg(self) -> i8 { -self } }
impl Not for i8 { type Output = i8; fn not(self) -> i8 { !self } }
impl Add for i16 { type Output = i16; #[rustc_inherit_overflow_checks] fn add(self, rhs: i16) -> i16 { self + rhs } }
impl AddAssign for i16 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: i16) { *self += rhs; } }
impl Sub for i16 { type Output = i16; #[rustc_inherit_overflow_checks] fn sub(self, rhs: i16) -> i16 { self - rhs } }
impl SubAssign for i16 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: i16) { *self -= rhs; } }
impl Mul for i16 { type Output = i16; #[rustc_inherit_overflow_checks] fn mul(self, rhs: i16) -> i16 { self * rhs } }
impl MulAssign for i16 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: i16) { *self *= rhs; } }
impl Div for i16 { type Output = i16; #[rustc_inherit_overflow_checks] fn div(self, rhs: i16) -> i16 { self / rhs } }
impl DivAssign for i16 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: i16) { *self /= rhs; } }
impl Rem for i16 { type Output = i16; #[rustc_inherit_overflow_checks] fn rem(self, rhs: i16) -> i16 { self % rhs } }
impl RemAssign for i16 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: i16) { *self %= rhs; } }
impl BitAnd for i16 { type Output = i16; fn bitand(self, rhs: i16) -> i16 { self & rhs } }
impl BitAndAssign for i16 { fn bitand_assign(&mut self, rhs: i16) { *self &= rhs; } }
impl BitOr for i16 { type Output = i16; fn bitor(self, rhs: i16) -> i16 { self | rhs } }
impl BitOrAssign for i16 { fn bitor_assign(&mut self, rhs: i16) { *self |= rhs; } }
impl BitXor for i16 { type Output = i16; fn bitxor(self, rhs: i16) -> i16 { self ^ rhs } }
impl BitXorAssign for i16 { fn bitxor_assign(&mut self, rhs: i16) { *self ^= rhs; } }
impl Neg for i16 { type Output = i16; #[rustc_inherit_overflow_checks] fn neg(self) -> i16 { -self } }
impl Not for i16 { type Output = i16; fn not(self) -> i16 { !self } }
impl Add for i32 { type Output = i32; #[rustc_inherit_overflow_checks] fn add(self, rhs: i32) -> i32 { self + rhs } }
impl AddAssign for i32 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: i32) { *self += rhs; } }
impl Sub for i32 { type Output = i32; #[rustc_inherit_overflow_checks] fn sub(self, rhs: i32) -> i32 { self - rhs } }
impl SubAssign for i32 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: i32) { *self -= rhs; } }
impl Mul for i32 { type Output = i32; #[rustc_inherit_overflow_checks] fn mul(self, rhs: i32) -> i32 { self * rhs } }
impl MulAssign for i32 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: i32) { *self *= rhs; } }
impl Div for i32 { type Output = i32; #[rustc_inherit_overflow_checks] fn div(self, rhs: i32) -> i32 { self / rhs } }
impl DivAssign for i32 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: i32) { *self /= rhs; } }
impl Rem for i32 { type Output = i32; #[rustc_inherit_overflow_checks] fn rem(self, rhs: i32) -> i32 { self % rhs } }
impl RemAssign for i32 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: i32) { *self %= rhs; } }
impl BitAnd for i32 { type Output = i32; fn bitand(self, rhs: i32) -> i32 { self & rhs } }
impl BitAndAssign for i32 { fn bitand_assign(&mut self, rhs: i32) { *self &= rhs; } }
impl BitOr for i32 { type Output = i32; fn bitor(self, rhs: i32) -> i32 { self | rhs } }
impl BitOrAssign for i32 { fn bitor_assign(&mut self, rhs: i32) { *self |= rhs; } }
impl BitXor for i32 { type Output = i32; fn bitxor(self, rhs: i32) -> i32 { self ^ rhs } }
impl BitXorAssign for i32 { fn bitxor_assign(&mut self, rhs: i32) { *self ^= rhs; } }
impl Neg for i32 { type Output = i32; #[rustc_inherit_overflow_checks] fn neg(self) -> i32 { -self } }
impl Not for i32 { type Output = i32; fn not(self) -> i32 { !self } }
impl Add for i64 { type Output = i64; #[rustc_inherit_overflow_checks] fn add(self, rhs: i64) -> i64 { self + rhs } }
impl AddAssign for i64 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: i64) { *self += rhs; } }
impl Sub for i64 { type Output = i64; #[rustc_inherit_overflow_checks] fn sub(self, rhs: i64) -> i64 { self - rhs } }
impl SubAssign for i64 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: i64) { *self -= rhs; } }
impl Mul for i64 { type Output = i64; #[rustc_inherit_overflow_checks] fn mul(self, rhs: i64) -> i64 { self * rhs } }
impl MulAssign for i64 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: i64) { *self *= rhs; } }
impl Div for i64 { type Output = i64; #[rustc_inherit_overflow_checks] fn div(self, rhs: i64) -> i64 { self / rhs } }
impl DivAssign for i64 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: i64) { *self /= rhs; } }
impl Rem for i64 { type Output = i64; #[rustc_inherit_overflow_checks] fn rem(self, rhs: i64) -> i64 { self % rhs } }
impl RemAssign for i64 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: i64) { *self %= rhs; } }
impl BitAnd for i64 { type Output = i64; fn bitand(self, rhs: i64) -> i64 { self & rhs } }
impl BitAndAssign for i64 { fn bitand_assign(&mut self, rhs: i64) { *self &= rhs; } }
impl BitOr for i64 { type Output = i64; fn bitor(self, rhs: i64) -> i64 { self | rhs } }
impl BitOrAssign for i64 { fn bitor_assign(&mut self, rhs: i64) { *self |= rhs; } }
impl BitXor for i64 { type Output = i64; fn bitxor(self, rhs: i64) -> i64 { self ^ rhs } }
impl BitXorAssign for i64 { fn bitxor_assign(&mut self, rhs: i64) { *self ^= rhs; } }
impl Neg for i64 { type Output = i64; #[rustc_inherit_overflow_checks] fn neg(self) -> i64 { -self } }
impl Not for i64 { type Output = i64; fn not(self) -> i64 { !self } }
impl Add for i128 { type Output = i128; #[rustc_inherit_overflow_checks] fn add(self, rhs: i128) -> i128 { self + rhs } }
impl AddAssign for i128 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: i128) { *self += rhs; } }
impl Sub for i128 { type Output = i128; #[rustc_inherit_overflow_checks] fn sub(self, rhs: i128) -> i128 { self - rhs } }
impl SubAssign for i128 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: i128) { *self -= rhs; } }
impl Mul for i128 { type Output = i128; #[rustc_inherit_overflow_checks] fn mul(self, rhs: i128) -> i128 { self * rhs } }
impl MulAssign for i128 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: i128) { *self *= rhs; } }
impl Div for i128 { type Output = i128; #[rustc_inherit_overflow_checks] fn div(self, rhs: i128) -> i128 { self / rhs } }
impl DivAssign for i128 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: i128) { *self /= rhs; } }
impl Rem for i128 { type Output = i128; #[rustc_inherit_overflow_checks] fn rem(self, rhs: i128) -> i128 { self % rhs } }
impl RemAssign for i128 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: i128) { *self %= rhs; } }
impl BitAnd for i128 { type Output = i128; fn bitand(self, rhs: i128) -> i128 { self & rhs } }
impl BitAndAssign for i128 { fn bitand_assign(&mut self, rhs: i128) { *self &= rhs; } }
impl BitOr for i128 { type Output = i128; fn bitor(self, rhs: i128) -> i128 { self | rhs } }
impl BitOrAssign for i128 { fn bitor_assign(&mut self, rhs: i128) { *self |= rhs; } }
impl BitXor for i128 { type Output = i128; fn bitxor(self, rhs: i128) -> i128 { self ^ rhs } }
impl BitXorAssign for i128 { fn bitxor_assign(&mut self, rhs: i128) { *self ^= rhs; } }
impl Neg for i128 { type Output = i128; #[rustc_inherit_overflow_checks] fn neg(self) -> i128 { -self } }
impl Not for i128 { type Output = i128; fn not(self) -> i128 { !self } }
impl Add for isize { type Output = isize; #[rustc_inherit_overflow_checks] fn add(self, rhs: isize) -> isize { self + rhs } }
impl AddAssign for isize { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: isize) { *self += rhs; } }
impl Sub for isize { type Output = isize; #[rustc_inherit_overflow_checks] fn sub(self, rhs: isize) -> isize { self - rhs } }
impl SubAssign for isize { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: isize) { *self -= rhs; } }
impl Mul for isize { type Output = isize; #[rustc_inherit_overflow_checks] fn mul(self, rhs: isize) -> isize { self * rhs } }
impl MulAssign for isize { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: isize) { *self *= rhs; } }
impl Div for isize { type Output = isize; #[rustc_inherit_overflow_checks] fn div(self, rhs: isize) -> isize { self / rhs } }
impl DivAssign for isize { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: isize) { *self /= rhs; } }
impl Rem for isize { type Output = isize; #[rustc_inherit_overflow_checks] fn rem(self, rhs: isize) -> isize { self % rhs } }
impl RemAssign for isize { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: isize) { *self %= rhs; } }
impl BitAnd for isize { type Output = isize; fn bitand(self, rhs: isize) -> isize { self & rhs } }
impl BitAndAssign for isize { fn bitand_assign(&mut self, rhs: isize) { *self &= rhs; } }
impl BitOr for isize { type Output = isize; fn bitor(self, rhs: isize) -> isize { self | rhs } }
impl BitOrAssign for isize { fn bitor_assign(&mut self, rhs: isize) { *self |= rhs; } }
impl BitXor for isize { type Output = isize; fn bitxor(self, rhs: isize) -> isize { self ^ rhs } }
impl BitXorAssign for isize { fn bitxor_assign(&mut self, rhs: isize) { *self ^= rhs; } }
impl Neg for isize { type Output = isize; #[rustc_inherit_overflow_checks] fn neg(self) -> isize { -self } }
impl Not for isize { type Output = isize; fn not(self) -> isize { !self } }
impl Add for u8 { type Output = u8; #[rustc_inherit_overflow_checks] fn add(self, rhs: u8) -> u8 { self + rhs } }
impl AddAssign for u8 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: u8) { *self += rhs; } }
impl Sub for u8 { type Output = u8; #[rustc_inherit_overflow_checks] fn sub(self, rhs: u8) -> u8 { self - rhs } }
impl SubAssign for u8 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: u8) { *self -= rhs; } }
impl Mul for u8 { type Output = u8; #[rustc_inherit_overflow_checks] fn mul(self, rhs: u8) -> u8 { self * rhs } }
impl MulAssign for u8 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: u8) { *self *= rhs; } }
impl Div for u8 { type Output = u8; #[rustc_inherit_overflow_checks] fn div(self, rhs: u8) -> u8 { self / rhs } }
impl DivAssign for u8 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: u8) { *self /= rhs; } }
impl Rem for u8 { type Output = u8; #[rustc_inherit_overflow_checks] fn rem(self, rhs: u8) -> u8 { self % rhs } }
impl RemAssign for u8 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: u8) { *self %= rhs; } }
impl BitAnd for u8 { type Output = u8; fn bitand(self, rhs: u8) -> u8 { self & rhs } }
impl BitAndAssign for u8 { fn bitand_assign(&mut self, rhs: u8) { *self &= rhs; } }
impl BitOr for u8 { type Output = u8; fn bitor(self, rhs: u8) -> u8 { self | rhs } }
impl BitOrAssign for u8 { fn bitor_assign(&mut self, rhs: u8) { *self |= rhs; } }
impl BitXor for u8 { type Output = u8; fn bitxor(self, rhs: u8) -> u8 { self ^ rhs } }
impl BitXorAssign for u8 { fn bitxor_assign(&mut self, rhs: u8) { *self ^= rhs; } }
impl Not for u8 { type Output = u8; fn not(self) -> u8 { !self } }
impl Add for u16 { type Output = u16; #[rustc_inherit_overflow_checks] fn add(self, rhs: u16) -> u16 { self + rhs } }
impl AddAssign for u16 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: u16) { *self += rhs; } }
impl Sub for u16 { type Output = u16; #[rustc_inherit_overflow_checks] fn sub(self, rhs: u16) -> u16 { self - rhs } }
impl SubAssign for u16 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: u16) { *self -= rhs; } }
impl Mul for u16 { type Output = u16; #[rustc_inherit_overflow_checks] fn mul(self, rhs: u16) -> u16 { self * rhs } }
impl MulAssign for u16 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: u16) { *self *= rhs; } }
impl Div for u16 { type Output = u16; #[rustc_inherit_overflow_checks] fn div(self, rhs: u16) -> u16 { self / rhs } }
impl DivAssign for u16 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: u16) { *self /= rhs; } }
impl Rem for u16 { type Output = u16; #[rustc_inherit_overflow_checks] fn rem(self, rhs: u16) -> u16 { self % rhs } }
impl RemAssign for u16 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: u16) { *self %= rhs; } }
impl BitAnd for u16 { type Output = u16; fn bitand(self, rhs: u16) -> u16 { self & rhs } }
impl BitAndAssign for u16 { fn bitand_assign(&mut self, rhs: u16) { *self &= rhs; } }
impl BitOr for u16 { type Output = u16; fn bitor(self, rhs: u16) -> u16 { self | rhs } }
impl BitOrAssign for u16 { fn bitor_assign(&mut self, rhs: u16) { *self |= rhs; } }
impl BitXor for u16 { type Output = u16; fn bitxor(self, rhs: u16) -> u16 { self ^ rhs } }
impl BitXorAssign for u16 { fn bitxor_assign(&mut self, rhs: u16) { *self ^= rhs; } }
impl Not for u16 { type Output = u16; fn not(self) -> u16 { !self } }
impl Add for u32 { type Output = u32; #[rustc_inherit_overflow_checks] fn add(self, rhs: u32) -> u32 { self + rhs } }
impl AddAssign for u32 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: u32) { *self += rhs; } }
impl Sub for u32 { type Output = u32; #[rustc_inherit_overflow_checks] fn sub(self, rhs: u32) -> u32 { self - rhs } }
impl SubAssign for u32 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: u32) { *self -= rhs; } }
impl Mul for u32 { type Output = u32; #[rustc_inherit_overflow_checks] fn mul(self, rhs: u32) -> u32 { self * rhs } }
impl MulAssign for u32 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: u32) { *self *= rhs; } }
impl Div for u32 { type Output = u32; #[rustc_inherit_overflow_checks] fn div(self, rhs: u32) -> u32 { self / rhs } }
impl DivAssign for u32 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: u32) { *self /= rhs; } }
impl Rem for u32 { type Output = u32; #[rustc_inherit_overflow_checks] fn rem(self, rhs: u32) -> u32 { self % rhs } }
impl RemAssign for u32 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: u32) { *self %= rhs; } }
impl BitAnd for u32 { type Output = u32; fn bitand(self, rhs: u32) -> u32 { self & rhs } }
impl BitAndAssign for u32 { fn bitand_assign(&mut self, rhs: u32) { *self &= rhs; } }
impl BitOr for u32 { type Output = u32; fn bitor(self, rhs: u32) -> u32 { self | rhs } }
impl BitOrAssign for u32 { fn bitor_assign(&mut self, rhs: u32) { *self |= rhs; } }
impl BitXor for u32 { type Output = u32; fn bitxor(self, rhs: u32) -> u32 { self ^ rhs } }
impl BitXorAssign for u32 { fn bitxor_assign(&mut self, rhs: u32) { *self ^= rhs; } }
impl Not for u32 { type Output = u32; fn not(self) -> u32 { !self } }
impl Add for u64 { type Output = u64; #[rustc_inherit_overflow_checks] fn add(self, rhs: u64) -> u64 { self + rhs } }
impl AddAssign for u64 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: u64) { *self += rhs; } }
impl Sub for u64 { type Output = u64; #[rustc_inherit_overflow_checks] fn sub(self, rhs: u64) -> u64 { self - rhs } }
impl SubAssign for u64 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: u64) { *self -= rhs; } }
impl Mul for u64 { type Output = u64; #[rustc_inherit_overflow_checks] fn mul(self, rhs: u64) -> u64 { self * rhs } }
impl MulAssign for u64 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: u64) { *self *= rhs; } }
impl Div for u64 { type Output = u64; #[rustc_inherit_overflow_checks] fn div(self, rhs: u64) -> u64 { self / rhs } }
impl DivAssign for u64 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: u64) { *self /= rhs; } }
impl Rem for u64 { type Output = u64; #[rustc_inherit_overflow_checks] fn rem(self, rhs: u64) -> u64 { self % rhs } }
impl RemAssign for u64 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: u64) { *self %= rhs; } }
impl BitAnd for u64 { type Output = u64; fn bitand(self, rhs: u64) -> u64 { self & rhs } }
impl BitAndAssign for u64 { fn bitand_assign(&mut self, rhs: u64) { *self &= rhs; } }
impl BitOr for u64 { type Output = u64; fn bitor(self, rhs: u64) -> u64 { self | rhs } }
impl BitOrAssign for u64 { fn bitor_assign(&mut self, rhs: u64) { *self |= rhs; } }
impl BitXor for u64 { type Output = u64; fn bitxor(self, rhs: u64) -> u64 { self ^ rhs } }
impl BitXorAssign for u64 { fn bitxor_assign(&mut self, rhs: u64) { *self ^= rhs; } }
impl Not for u64 { type Output = u64; fn not(self) -> u64 { !self } }
impl Add for u128 { type Output = u128; #[rustc_inherit_overflow_checks] fn add(self, rhs: u128) -> u128 { self + rhs } }
impl AddAssign for u128 { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: u128) { *self += rhs; } }
impl Sub for u128 { type Output = u128; #[rustc_inherit_overflow_checks] fn sub(self, rhs: u128) -> u128 { self - rhs } }
impl SubAssign for u128 { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: u128) { *self -= rhs; } }
impl Mul for u128 { type Output = u128; #[rustc_inherit_overflow_checks] fn mul(self, rhs: u128) -> u128 { self * rhs } }
impl MulAssign for u128 { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: u128) { *self *= rhs; } }
impl Div for u128 { type Output = u128; #[rustc_inherit_overflow_checks] fn div(self, rhs: u128) -> u128 { self / rhs } }
impl DivAssign for u128 { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: u128) { *self /= rhs; } }
impl Rem for u128 { type Output = u128; #[rustc_inherit_overflow_checks] fn rem(self, rhs: u128) -> u128 { self % rhs } }
impl RemAssign for u128 { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: u128) { *self %= rhs; } }
impl BitAnd for u128 { type Output = u128; fn bitand(self, rhs: u128) -> u128 { self & rhs } }
impl BitAndAssign for u128 { fn bitand_assign(&mut self, rhs: u128) { *self &= rhs; } }
impl BitOr for u128 { type Output = u128; fn bitor(self, rhs: u128) -> u128 { self | rhs } }
impl BitOrAssign for u128 { fn bitor_assign(&mut self, rhs: u128) { *self |= rhs; } }
impl BitXor for u128 { type Output = u128; fn bitxor(self, rhs: u128) -> u128 { self ^ rhs } }
impl BitXorAssign for u128 { fn bitxor_assign(&mut self, rhs: u128) { *self ^= rhs; } }
impl Not for u128 { type Output = u128; fn not(self) -> u128 { !self } }
impl Add for usize { type Output = usize; #[rustc_inherit_overflow_checks] fn add(self, rhs: usize) -> usize { self + rhs } }
impl AddAssign for usize { #[rustc_inherit_overflow_checks] fn add_assign(&mut self, rhs: usize) { *self += rhs; } }
impl Sub for usize { type Output = usize; #[rustc_inherit_overflow_checks] fn sub(self, rhs: usize) -> usize { self - rhs } }
impl SubAssign for usize { #[rustc_inherit_overflow_checks] fn sub_assign(&mut self, rhs: usize) { *self -= rhs; } }
impl Mul for usize { type Output = usize; #[rustc_inherit_overflow_checks] fn mul(self, rhs: usize) -> usize { self * rhs } }
impl MulAssign for usize { #[rustc_inherit_overflow_checks] fn mul_assign(&mut self, rhs: usize) { *self *= rhs; } }
impl Div for usize { type Output = usize; #[rustc_inherit_overflow_checks] fn div(self, rhs: usize) -> usize { self / rhs } }
impl DivAssign for usize { #[rustc_inherit_overflow_checks] fn div_assign(&mut self, rhs: usize) { *self /= rhs; } }
impl Rem for usize { type Output = usize; #[rustc_inherit_overflow_checks] fn rem(self, rhs: usize) -> usize { self % rhs } }
impl RemAssign for usize { #[rustc_inherit_overflow_checks] fn rem_assign(&mut self, rhs: usize) { *self %= rhs; } }
impl BitAnd for usize { type Output = usize; fn bitand(self, rhs: usize) -> usize { self & rhs } }
impl BitAndAssign for usize { fn bitand_assign(&mut self, rhs: usize) { *self &= rhs; } }
impl BitOr for usize { type Output = usize; fn bitor(self, rhs: usize) -> usize { self | rhs } }
impl BitOrAssign for usize { fn bitor_assign(&mut self, rhs: usize) { *self |= rhs; } }
impl BitXor for usize { type Output = usize; fn bitxor(self, rhs: usize) -> usize { self ^ rhs } }
impl BitXorAssign for usize { fn bitxor_assign(&mut self, rhs: usize) { *self ^= rhs; } }
impl Not for usize { type Output = usize; fn not(self) -> usize { !self } }
impl BitAnd for bool { type Output = bool; fn bitand(self, rhs: bool) -> bool { self & rhs } }
impl BitAndAssign for bool { fn bitand_assign(&mut self, rhs: bool) { *self &= rhs; } }
impl BitOr for bool { type Output = bool; fn bitor(self, rhs: bool) -> bool { self | rhs } }
impl BitOrAssign for bool { fn bitor_assign(&mut self, rhs: bool) { *self |= rhs; } }
impl BitXor for bool { type Output = bool; fn bitxor(self, rhs: bool) -> bool { self ^ rhs } }
impl BitXorAssign for bool { fn bitxor_assign(&mut self, rhs: bool) { *self ^= rhs; } }
impl Not for bool { type Output = bool; fn not(self) -> bool { !self } }

/// The integer types and references to them, as shift amounts: rustc's
/// `core` implements `Shl` and `Shr` for every pair of integer types,
/// each by value and by reference, which this trait lets one `impl`
/// block per type and operator do. Programs cannot name it.
pub(crate) trait ShiftAmount {
    /// The amount as a `u32`, whose low bits, which are all a shift
    /// uses, are the amount's.
    fn amount(self) -> u32;
}

impl ShiftAmount for i8 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &i8 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for i16 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &i16 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for i32 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &i32 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for i64 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &i64 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for i128 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &i128 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for isize { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &isize { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for u8 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &u8 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for u16 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &u16 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for u32 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &u32 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for u64 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &u64 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for u128 { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &u128 { fn amount(self) -> u32 { *self as u32 } }
impl ShiftAmount for usize { fn amount(self) -> u32 { self as u32 } }
impl ShiftAmount for &usize { fn amount(self) -> u32 { *self as u32 } }

impl<R: ShiftAmount> Shl<R> for i8 { type Output = i8; fn shl(self, rhs: R) -> i8 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &i8 { type Output = i8; fn shl(self, rhs: R) -> i8 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for i8 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for i8 { type Output = i8; fn shr(self, rhs: R) -> i8 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &i8 { type Output = i8; fn shr(self, rhs: R) -> i8 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for i8 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for i16 { type Output = i16; fn shl(self, rhs: R) -> i16 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &i16 { type Output = i16; fn shl(self, rhs: R) -> i16 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for i16 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for i16 { type Output = i16; fn shr(self, rhs: R) -> i16 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &i16 { type Output = i16; fn shr(self, rhs: R) -> i16 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for i16 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for i32 { type Output = i32; fn shl(self, rhs: R) -> i32 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &i32 { type Output = i32; fn shl(self, rhs: R) -> i32 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for i32 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for i32 { type Output = i32; fn shr(self, rhs: R) -> i32 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &i32 { type Output = i32; fn shr(self, rhs: R) -> i32 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for i32 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for i64 { type Output = i64; fn shl(self, rhs: R) -> i64 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &i64 { type Output = i64; fn shl(self, rhs: R) -> i64 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for i64 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for i64 { type Output = i64; fn shr(self, rhs: R) -> i64 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &i64 { type Output = i64; fn shr(self, rhs: R) -> i64 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for i64 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for i128 { type Output = i128; fn shl(self, rhs: R) -> i128 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &i128 { type Output = i128; fn shl(self, rhs: R) -> i128 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for i128 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for i128 { type Output = i128; fn shr(self, rhs: R) -> i128 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &i128 { type Output = i128; fn shr(self, rhs: R) -> i128 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for i128 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for isize { type Output = isize; fn shl(self, rhs: R) -> isize { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &isize { type Output = isize; fn shl(self, rhs: R) -> isize { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for isize { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for isize { type Output = isize; fn shr(self, rhs: R) -> isize { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &isize { type Output = isize; fn shr(self, rhs: R) -> isize { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for isize { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for u8 { type Output = u8; fn shl(self, rhs: R) -> u8 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &u8 { type Output = u8; fn shl(self, rhs: R) -> u8 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for u8 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for u8 { type Output = u8; fn shr(self, rhs: R) -> u8 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &u8 { type Output = u8; fn shr(self, rhs: R) -> u8 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for u8 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for u16 { type Output = u16; fn shl(self, rhs: R) -> u16 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &u16 { type Output = u16; fn shl(self, rhs: R) -> u16 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for u16 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for u16 { type Output = u16; fn shr(self, rhs: R) -> u16 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &u16 { type Output = u16; fn shr(self, rhs: R) -> u16 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for u16 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for u32 { type Output = u32; fn shl(self, rhs: R) -> u32 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &u32 { type Output = u32; fn shl(self, rhs: R) -> u32 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for u32 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for u32 { type Output = u32; fn shr(self, rhs: R) -> u32 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &u32 { type Output = u32; fn shr(self, rhs: R) -> u32 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for u32 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for u64 { type Output = u64; fn shl(self, rhs: R) -> u64 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &u64 { type Output = u64; fn shl(self, rhs: R) -> u64 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for u64 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for u64 { type Output = u64; fn shr(self, rhs: R) -> u64 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &u64 { type Output = u64; fn shr(self, rhs: R) -> u64 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for u64 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for u128 { type Output = u128; fn shl(self, rhs: R) -> u128 { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &u128 { type Output = u128; fn shl(self, rhs: R) -> u128 { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for u128 { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for u128 { type Output = u128; fn shr(self, rhs: R) -> u128 { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &u128 { type Output = u128; fn shr(self, rhs: R) -> u128 { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for u128 { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }
impl<R: ShiftAmount> Shl<R> for usize { type Output = usize; fn shl(self, rhs: R) -> usize { self << rhs.amount() } }
impl<R: ShiftAmount> Shl<R> for &usize { type Output = usize; fn shl(self, rhs: R) -> usize { *self << rhs.amount() } }
impl<R: ShiftAmount> ShlAssign<R> for usize { fn shl_assign(&mut self, rhs: R) { *self <<= rhs.amount(); } }
impl<R: ShiftAmount> Shr<R> for usize { type Output = usize; fn shr(self, rhs: R) -> usize { self >> rhs.amount() } }
impl<R: ShiftAmount> Shr<R> for &usize { type Output = usize; fn shr(self, rhs: R) -> usize { *self >> rhs.amount() } }
impl<R: ShiftAmount> ShrAssign<R> for usize { fn shr_assign(&mut self, rhs: R) { *self >>= rhs.amount(); } }

// Operators on references, `&a + b`, `a + &b`, `&a + &b`, `a += &b`,
// `-&a` and `!&a`, which work on the values, as rustc's
// `forward_ref_binop!` impls do.
impl Add<i8> for &i8 { type Output = i8; fn add(self, rhs: i8) -> i8 { *self + rhs } }
impl Add<&i8> for i8 { type Output = i8; fn add(self, rhs: &i8) -> i8 { self + *rhs } }
impl Add<&i8> for &i8 { type Output = i8; fn add(self, rhs: &i8) -> i8 { *self + *rhs } }
impl AddAssign<&i8> for i8 { fn add_assign(&mut self, rhs: &i8) { *self += *rhs; } }
impl Sub<i8> for &i8 { type Output = i8; fn sub(self, rhs: i8) -> i8 { *self - rhs } }
impl Sub<&i8> for i8 { type Output = i8; fn sub(self, rhs: &i8) -> i8 { self - *rhs } }
impl Sub<&i8> for &i8 { type Output = i8; fn sub(self, rhs: &i8) -> i8 { *self - *rhs } }
impl SubAssign<&i8> for i8 { fn sub_assign(&mut self, rhs: &i8) { *self -= *rhs; } }
impl Mul<i8> for &i8 { type Output = i8; fn mul(self, rhs: i8) -> i8 { *self * rhs } }
impl Mul<&i8> for i8 { type Output = i8; fn mul(self, rhs: &i8) -> i8 { self * *rhs } }
impl Mul<&i8> for &i8 { type Output = i8; fn mul(self, rhs: &i8) -> i8 { *self * *rhs } }
impl MulAssign<&i8> for i8 { fn mul_assign(&mut self, rhs: &i8) { *self *= *rhs; } }
impl Div<i8> for &i8 { type Output = i8; fn div(self, rhs: i8) -> i8 { *self / rhs } }
impl Div<&i8> for i8 { type Output = i8; fn div(self, rhs: &i8) -> i8 { self / *rhs } }
impl Div<&i8> for &i8 { type Output = i8; fn div(self, rhs: &i8) -> i8 { *self / *rhs } }
impl DivAssign<&i8> for i8 { fn div_assign(&mut self, rhs: &i8) { *self /= *rhs; } }
impl Rem<i8> for &i8 { type Output = i8; fn rem(self, rhs: i8) -> i8 { *self % rhs } }
impl Rem<&i8> for i8 { type Output = i8; fn rem(self, rhs: &i8) -> i8 { self % *rhs } }
impl Rem<&i8> for &i8 { type Output = i8; fn rem(self, rhs: &i8) -> i8 { *self % *rhs } }
impl RemAssign<&i8> for i8 { fn rem_assign(&mut self, rhs: &i8) { *self %= *rhs; } }
impl BitAnd<i8> for &i8 { type Output = i8; fn bitand(self, rhs: i8) -> i8 { *self & rhs } }
impl BitAnd<&i8> for i8 { type Output = i8; fn bitand(self, rhs: &i8) -> i8 { self & *rhs } }
impl BitAnd<&i8> for &i8 { type Output = i8; fn bitand(self, rhs: &i8) -> i8 { *self & *rhs } }
impl BitAndAssign<&i8> for i8 { fn bitand_assign(&mut self, rhs: &i8) { *self &= *rhs; } }
impl BitOr<i8> for &i8 { type Output = i8; fn bitor(self, rhs: i8) -> i8 { *self | rhs } }
impl BitOr<&i8> for i8 { type Output = i8; fn bitor(self, rhs: &i8) -> i8 { self | *rhs } }
impl BitOr<&i8> for &i8 { type Output = i8; fn bitor(self, rhs: &i8) -> i8 { *self | *rhs } }
impl BitOrAssign<&i8> for i8 { fn bitor_assign(&mut self, rhs: &i8) { *self |= *rhs; } }
impl BitXor<i8> for &i8 { type Output = i8; fn bitxor(self, rhs: i8) -> i8 { *self ^ rhs } }
impl BitXor<&i8> for i8 { type Output = i8; fn bitxor(self, rhs: &i8) -> i8 { self ^ *rhs } }
impl BitXor<&i8> for &i8 { type Output = i8; fn bitxor(self, rhs: &i8) -> i8 { *self ^ *rhs } }
impl BitXorAssign<&i8> for i8 { fn bitxor_assign(&mut self, rhs: &i8) { *self ^= *rhs; } }
impl Neg for &i8 { type Output = i8; fn neg(self) -> i8 { -*self } }
impl Not for &i8 { type Output = i8; fn not(self) -> i8 { !*self } }
impl Add<i16> for &i16 { type Output = i16; fn add(self, rhs: i16) -> i16 { *self + rhs } }
impl Add<&i16> for i16 { type Output = i16; fn add(self, rhs: &i16) -> i16 { self + *rhs } }
impl Add<&i16> for &i16 { type Output = i16; fn add(self, rhs: &i16) -> i16 { *self + *rhs } }
impl AddAssign<&i16> for i16 { fn add_assign(&mut self, rhs: &i16) { *self += *rhs; } }
impl Sub<i16> for &i16 { type Output = i16; fn sub(self, rhs: i16) -> i16 { *self - rhs } }
impl Sub<&i16> for i16 { type Output = i16; fn sub(self, rhs: &i16) -> i16 { self - *rhs } }
impl Sub<&i16> for &i16 { type Output = i16; fn sub(self, rhs: &i16) -> i16 { *self - *rhs } }
impl SubAssign<&i16> for i16 { fn sub_assign(&mut self, rhs: &i16) { *self -= *rhs; } }
impl Mul<i16> for &i16 { type Output = i16; fn mul(self, rhs: i16) -> i16 { *self * rhs } }
impl Mul<&i16> for i16 { type Output = i16; fn mul(self, rhs: &i16) -> i16 { self * *rhs } }
impl Mul<&i16> for &i16 { type Output = i16; fn mul(self, rhs: &i16) -> i16 { *self * *rhs } }
impl MulAssign<&i16> for i16 { fn mul_assign(&mut self, rhs: &i16) { *self *= *rhs; } }
impl Div<i16> for &i16 { type Output = i16; fn div(self, rhs: i16) -> i16 { *self / rhs } }
impl Div<&i16> for i16 { type Output = i16; fn div(self, rhs: &i16) -> i16 { self / *rhs } }
impl Div<&i16> for &i16 { type Output = i16; fn div(self, rhs: &i16) -> i16 { *self / *rhs } }
impl DivAssign<&i16> for i16 { fn div_assign(&mut self, rhs: &i16) { *self /= *rhs; } }
impl Rem<i16> for &i16 { type Output = i16; fn rem(self, rhs: i16) -> i16 { *self % rhs } }
impl Rem<&i16> for i16 { type Output = i16; fn rem(self, rhs: &i16) -> i16 { self % *rhs } }
impl Rem<&i16> for &i16 { type Output = i16; fn rem(self, rhs: &i16) -> i16 { *self % *rhs } }
impl RemAssign<&i16> for i16 { fn rem_assign(&mut self, rhs: &i16) { *self %= *rhs; } }
impl BitAnd<i16> for &i16 { type Output = i16; fn bitand(self, rhs: i16) -> i16 { *self & rhs } }
impl BitAnd<&i16> for i16 { type Output = i16; fn bitand(self, rhs: &i16) -> i16 { self & *rhs } }
impl BitAnd<&i16> for &i16 { type Output = i16; fn bitand(self, rhs: &i16) -> i16 { *self & *rhs } }
impl BitAndAssign<&i16> for i16 { fn bitand_assign(&mut self, rhs: &i16) { *self &= *rhs; } }
impl BitOr<i16> for &i16 { type Output = i16; fn bitor(self, rhs: i16) -> i16 { *self | rhs } }
impl BitOr<&i16> for i16 { type Output = i16; fn bitor(self, rhs: &i16) -> i16 { self | *rhs } }
impl BitOr<&i16> for &i16 { type Output = i16; fn bitor(self, rhs: &i16) -> i16 { *self | *rhs } }
impl BitOrAssign<&i16> for i16 { fn bitor_assign(&mut self, rhs: &i16) { *self |= *rhs; } }
impl BitXor<i16> for &i16 { type Output = i16; fn bitxor(self, rhs: i16) -> i16 { *self ^ rhs } }
impl BitXor<&i16> for i16 { type Output = i16; fn bitxor(self, rhs: &i16) -> i16 { self ^ *rhs } }
impl BitXor<&i16> for &i16 { type Output = i16; fn bitxor(self, rhs: &i16) -> i16 { *self ^ *rhs } }
impl BitXorAssign<&i16> for i16 { fn bitxor_assign(&mut self, rhs: &i16) { *self ^= *rhs; } }
impl Neg for &i16 { type Output = i16; fn neg(self) -> i16 { -*self } }
impl Not for &i16 { type Output = i16; fn not(self) -> i16 { !*self } }
impl Add<i32> for &i32 { type Output = i32; fn add(self, rhs: i32) -> i32 { *self + rhs } }
impl Add<&i32> for i32 { type Output = i32; fn add(self, rhs: &i32) -> i32 { self + *rhs } }
impl Add<&i32> for &i32 { type Output = i32; fn add(self, rhs: &i32) -> i32 { *self + *rhs } }
impl AddAssign<&i32> for i32 { fn add_assign(&mut self, rhs: &i32) { *self += *rhs; } }
impl Sub<i32> for &i32 { type Output = i32; fn sub(self, rhs: i32) -> i32 { *self - rhs } }
impl Sub<&i32> for i32 { type Output = i32; fn sub(self, rhs: &i32) -> i32 { self - *rhs } }
impl Sub<&i32> for &i32 { type Output = i32; fn sub(self, rhs: &i32) -> i32 { *self - *rhs } }
impl SubAssign<&i32> for i32 { fn sub_assign(&mut self, rhs: &i32) { *self -= *rhs; } }
impl Mul<i32> for &i32 { type Output = i32; fn mul(self, rhs: i32) -> i32 { *self * rhs } }
impl Mul<&i32> for i32 { type Output = i32; fn mul(self, rhs: &i32) -> i32 { self * *rhs } }
impl Mul<&i32> for &i32 { type Output = i32; fn mul(self, rhs: &i32) -> i32 { *self * *rhs } }
impl MulAssign<&i32> for i32 { fn mul_assign(&mut self, rhs: &i32) { *self *= *rhs; } }
impl Div<i32> for &i32 { type Output = i32; fn div(self, rhs: i32) -> i32 { *self / rhs } }
impl Div<&i32> for i32 { type Output = i32; fn div(self, rhs: &i32) -> i32 { self / *rhs } }
impl Div<&i32> for &i32 { type Output = i32; fn div(self, rhs: &i32) -> i32 { *self / *rhs } }
impl DivAssign<&i32> for i32 { fn div_assign(&mut self, rhs: &i32) { *self /= *rhs; } }
impl Rem<i32> for &i32 { type Output = i32; fn rem(self, rhs: i32) -> i32 { *self % rhs } }
impl Rem<&i32> for i32 { type Output = i32; fn rem(self, rhs: &i32) -> i32 { self % *rhs } }
impl Rem<&i32> for &i32 { type Output = i32; fn rem(self, rhs: &i32) -> i32 { *self % *rhs } }
impl RemAssign<&i32> for i32 { fn rem_assign(&mut self, rhs: &i32) { *self %= *rhs; } }
impl BitAnd<i32> for &i32 { type Output = i32; fn bitand(self, rhs: i32) -> i32 { *self & rhs } }
impl BitAnd<&i32> for i32 { type Output = i32; fn bitand(self, rhs: &i32) -> i32 { self & *rhs } }
impl BitAnd<&i32> for &i32 { type Output = i32; fn bitand(self, rhs: &i32) -> i32 { *self & *rhs } }
impl BitAndAssign<&i32> for i32 { fn bitand_assign(&mut self, rhs: &i32) { *self &= *rhs; } }
impl BitOr<i32> for &i32 { type Output = i32; fn bitor(self, rhs: i32) -> i32 { *self | rhs } }
impl BitOr<&i32> for i32 { type Output = i32; fn bitor(self, rhs: &i32) -> i32 { self | *rhs } }
impl BitOr<&i32> for &i32 { type Output = i32; fn bitor(self, rhs: &i32) -> i32 { *self | *rhs } }
impl BitOrAssign<&i32> for i32 { fn bitor_assign(&mut self, rhs: &i32) { *self |= *rhs; } }
impl BitXor<i32> for &i32 { type Output = i32; fn bitxor(self, rhs: i32) -> i32 { *self ^ rhs } }
impl BitXor<&i32> for i32 { type Output = i32; fn bitxor(self, rhs: &i32) -> i32 { self ^ *rhs } }
impl BitXor<&i32> for &i32 { type Output = i32; fn bitxor(self, rhs: &i32) -> i32 { *self ^ *rhs } }
impl BitXorAssign<&i32> for i32 { fn bitxor_assign(&mut self, rhs: &i32) { *self ^= *rhs; } }
impl Neg for &i32 { type Output = i32; fn neg(self) -> i32 { -*self } }
impl Not for &i32 { type Output = i32; fn not(self) -> i32 { !*self } }
impl Add<i64> for &i64 { type Output = i64; fn add(self, rhs: i64) -> i64 { *self + rhs } }
impl Add<&i64> for i64 { type Output = i64; fn add(self, rhs: &i64) -> i64 { self + *rhs } }
impl Add<&i64> for &i64 { type Output = i64; fn add(self, rhs: &i64) -> i64 { *self + *rhs } }
impl AddAssign<&i64> for i64 { fn add_assign(&mut self, rhs: &i64) { *self += *rhs; } }
impl Sub<i64> for &i64 { type Output = i64; fn sub(self, rhs: i64) -> i64 { *self - rhs } }
impl Sub<&i64> for i64 { type Output = i64; fn sub(self, rhs: &i64) -> i64 { self - *rhs } }
impl Sub<&i64> for &i64 { type Output = i64; fn sub(self, rhs: &i64) -> i64 { *self - *rhs } }
impl SubAssign<&i64> for i64 { fn sub_assign(&mut self, rhs: &i64) { *self -= *rhs; } }
impl Mul<i64> for &i64 { type Output = i64; fn mul(self, rhs: i64) -> i64 { *self * rhs } }
impl Mul<&i64> for i64 { type Output = i64; fn mul(self, rhs: &i64) -> i64 { self * *rhs } }
impl Mul<&i64> for &i64 { type Output = i64; fn mul(self, rhs: &i64) -> i64 { *self * *rhs } }
impl MulAssign<&i64> for i64 { fn mul_assign(&mut self, rhs: &i64) { *self *= *rhs; } }
impl Div<i64> for &i64 { type Output = i64; fn div(self, rhs: i64) -> i64 { *self / rhs } }
impl Div<&i64> for i64 { type Output = i64; fn div(self, rhs: &i64) -> i64 { self / *rhs } }
impl Div<&i64> for &i64 { type Output = i64; fn div(self, rhs: &i64) -> i64 { *self / *rhs } }
impl DivAssign<&i64> for i64 { fn div_assign(&mut self, rhs: &i64) { *self /= *rhs; } }
impl Rem<i64> for &i64 { type Output = i64; fn rem(self, rhs: i64) -> i64 { *self % rhs } }
impl Rem<&i64> for i64 { type Output = i64; fn rem(self, rhs: &i64) -> i64 { self % *rhs } }
impl Rem<&i64> for &i64 { type Output = i64; fn rem(self, rhs: &i64) -> i64 { *self % *rhs } }
impl RemAssign<&i64> for i64 { fn rem_assign(&mut self, rhs: &i64) { *self %= *rhs; } }
impl BitAnd<i64> for &i64 { type Output = i64; fn bitand(self, rhs: i64) -> i64 { *self & rhs } }
impl BitAnd<&i64> for i64 { type Output = i64; fn bitand(self, rhs: &i64) -> i64 { self & *rhs } }
impl BitAnd<&i64> for &i64 { type Output = i64; fn bitand(self, rhs: &i64) -> i64 { *self & *rhs } }
impl BitAndAssign<&i64> for i64 { fn bitand_assign(&mut self, rhs: &i64) { *self &= *rhs; } }
impl BitOr<i64> for &i64 { type Output = i64; fn bitor(self, rhs: i64) -> i64 { *self | rhs } }
impl BitOr<&i64> for i64 { type Output = i64; fn bitor(self, rhs: &i64) -> i64 { self | *rhs } }
impl BitOr<&i64> for &i64 { type Output = i64; fn bitor(self, rhs: &i64) -> i64 { *self | *rhs } }
impl BitOrAssign<&i64> for i64 { fn bitor_assign(&mut self, rhs: &i64) { *self |= *rhs; } }
impl BitXor<i64> for &i64 { type Output = i64; fn bitxor(self, rhs: i64) -> i64 { *self ^ rhs } }
impl BitXor<&i64> for i64 { type Output = i64; fn bitxor(self, rhs: &i64) -> i64 { self ^ *rhs } }
impl BitXor<&i64> for &i64 { type Output = i64; fn bitxor(self, rhs: &i64) -> i64 { *self ^ *rhs } }
impl BitXorAssign<&i64> for i64 { fn bitxor_assign(&mut self, rhs: &i64) { *self ^= *rhs; } }
impl Neg for &i64 { type Output = i64; fn neg(self) -> i64 { -*self } }
impl Not for &i64 { type Output = i64; fn not(self) -> i64 { !*self } }
impl Add<i128> for &i128 { type Output = i128; fn add(self, rhs: i128) -> i128 { *self + rhs } }
impl Add<&i128> for i128 { type Output = i128; fn add(self, rhs: &i128) -> i128 { self + *rhs } }
impl Add<&i128> for &i128 { type Output = i128; fn add(self, rhs: &i128) -> i128 { *self + *rhs } }
impl AddAssign<&i128> for i128 { fn add_assign(&mut self, rhs: &i128) { *self += *rhs; } }
impl Sub<i128> for &i128 { type Output = i128; fn sub(self, rhs: i128) -> i128 { *self - rhs } }
impl Sub<&i128> for i128 { type Output = i128; fn sub(self, rhs: &i128) -> i128 { self - *rhs } }
impl Sub<&i128> for &i128 { type Output = i128; fn sub(self, rhs: &i128) -> i128 { *self - *rhs } }
impl SubAssign<&i128> for i128 { fn sub_assign(&mut self, rhs: &i128) { *self -= *rhs; } }
impl Mul<i128> for &i128 { type Output = i128; fn mul(self, rhs: i128) -> i128 { *self * rhs } }
impl Mul<&i128> for i128 { type Output = i128; fn mul(self, rhs: &i128) -> i128 { self * *rhs } }
impl Mul<&i128> for &i128 { type Output = i128; fn mul(self, rhs: &i128) -> i128 { *self * *rhs } }
impl MulAssign<&i128> for i128 { fn mul_assign(&mut self, rhs: &i128) { *self *= *rhs; } }
impl Div<i128> for &i128 { type Output = i128; fn div(self, rhs: i128) -> i128 { *self / rhs } }
impl Div<&i128> for i128 { type Output = i128; fn div(self, rhs: &i128) -> i128 { self / *rhs } }
impl Div<&i128> for &i128 { type Output = i128; fn div(self, rhs: &i128) -> i128 { *self / *rhs } }
impl DivAssign<&i128> for i128 { fn div_assign(&mut self, rhs: &i128) { *self /= *rhs; } }
impl Rem<i128> for &i128 { type Output = i128; fn rem(self, rhs: i128) -> i128 { *self % rhs } }
impl Rem<&i128> for i128 { type Output = i128; fn rem(self, rhs: &i128) -> i128 { self % *rhs } }
impl Rem<&i128> for &i128 { type Output = i128; fn rem(self, rhs: &i128) -> i128 { *self % *rhs } }
impl RemAssign<&i128> for i128 { fn rem_assign(&mut self, rhs: &i128) { *self %= *rhs; } }
impl BitAnd<i128> for &i128 { type Output = i128; fn bitand(self, rhs: i128) -> i128 { *self & rhs } }
impl BitAnd<&i128> for i128 { type Output = i128; fn bitand(self, rhs: &i128) -> i128 { self & *rhs } }
impl BitAnd<&i128> for &i128 { type Output = i128; fn bitand(self, rhs: &i128) -> i128 { *self & *rhs } }
impl BitAndAssign<&i128> for i128 { fn bitand_assign(&mut self, rhs: &i128) { *self &= *rhs; } }
impl BitOr<i128> for &i128 { type Output = i128; fn bitor(self, rhs: i128) -> i128 { *self | rhs } }
impl BitOr<&i128> for i128 { type Output = i128; fn bitor(self, rhs: &i128) -> i128 { self | *rhs } }
impl BitOr<&i128> for &i128 { type Output = i128; fn bitor(self, rhs: &i128) -> i128 { *self | *rhs } }
impl BitOrAssign<&i128> for i128 { fn bitor_assign(&mut self, rhs: &i128) { *self |= *rhs; } }
impl BitXor<i128> for &i128 { type Output = i128; fn bitxor(self, rhs: i128) -> i128 { *self ^ rhs } }
impl BitXor<&i128> for i128 { type Output = i128; fn bitxor(self, rhs: &i128) -> i128 { self ^ *rhs } }
impl BitXor<&i128> for &i128 { type Output = i128; fn bitxor(self, rhs: &i128) -> i128 { *self ^ *rhs } }
impl BitXorAssign<&i128> for i128 { fn bitxor_assign(&mut self, rhs: &i128) { *self ^= *rhs; } }
impl Neg for &i128 { type Output = i128; fn neg(self) -> i128 { -*self } }
impl Not for &i128 { type Output = i128; fn not(self) -> i128 { !*self } }
impl Add<isize> for &isize { type Output = isize; fn add(self, rhs: isize) -> isize { *self + rhs } }
impl Add<&isize> for isize { type Output = isize; fn add(self, rhs: &isize) -> isize { self + *rhs } }
impl Add<&isize> for &isize { type Output = isize; fn add(self, rhs: &isize) -> isize { *self + *rhs } }
impl AddAssign<&isize> for isize { fn add_assign(&mut self, rhs: &isize) { *self += *rhs; } }
impl Sub<isize> for &isize { type Output = isize; fn sub(self, rhs: isize) -> isize { *self - rhs } }
impl Sub<&isize> for isize { type Output = isize; fn sub(self, rhs: &isize) -> isize { self - *rhs } }
impl Sub<&isize> for &isize { type Output = isize; fn sub(self, rhs: &isize) -> isize { *self - *rhs } }
impl SubAssign<&isize> for isize { fn sub_assign(&mut self, rhs: &isize) { *self -= *rhs; } }
impl Mul<isize> for &isize { type Output = isize; fn mul(self, rhs: isize) -> isize { *self * rhs } }
impl Mul<&isize> for isize { type Output = isize; fn mul(self, rhs: &isize) -> isize { self * *rhs } }
impl Mul<&isize> for &isize { type Output = isize; fn mul(self, rhs: &isize) -> isize { *self * *rhs } }
impl MulAssign<&isize> for isize { fn mul_assign(&mut self, rhs: &isize) { *self *= *rhs; } }
impl Div<isize> for &isize { type Output = isize; fn div(self, rhs: isize) -> isize { *self / rhs } }
impl Div<&isize> for isize { type Output = isize; fn div(self, rhs: &isize) -> isize { self / *rhs } }
impl Div<&isize> for &isize { type Output = isize; fn div(self, rhs: &isize) -> isize { *self / *rhs } }
impl DivAssign<&isize> for isize { fn div_assign(&mut self, rhs: &isize) { *self /= *rhs; } }
impl Rem<isize> for &isize { type Output = isize; fn rem(self, rhs: isize) -> isize { *self % rhs } }
impl Rem<&isize> for isize { type Output = isize; fn rem(self, rhs: &isize) -> isize { self % *rhs } }
impl Rem<&isize> for &isize { type Output = isize; fn rem(self, rhs: &isize) -> isize { *self % *rhs } }
impl RemAssign<&isize> for isize { fn rem_assign(&mut self, rhs: &isize) { *self %= *rhs; } }
impl BitAnd<isize> for &isize { type Output = isize; fn bitand(self, rhs: isize) -> isize { *self & rhs } }
impl BitAnd<&isize> for isize { type Output = isize; fn bitand(self, rhs: &isize) -> isize { self & *rhs } }
impl BitAnd<&isize> for &isize { type Output = isize; fn bitand(self, rhs: &isize) -> isize { *self & *rhs } }
impl BitAndAssign<&isize> for isize { fn bitand_assign(&mut self, rhs: &isize) { *self &= *rhs; } }
impl BitOr<isize> for &isize { type Output = isize; fn bitor(self, rhs: isize) -> isize { *self | rhs } }
impl BitOr<&isize> for isize { type Output = isize; fn bitor(self, rhs: &isize) -> isize { self | *rhs } }
impl BitOr<&isize> for &isize { type Output = isize; fn bitor(self, rhs: &isize) -> isize { *self | *rhs } }
impl BitOrAssign<&isize> for isize { fn bitor_assign(&mut self, rhs: &isize) { *self |= *rhs; } }
impl BitXor<isize> for &isize { type Output = isize; fn bitxor(self, rhs: isize) -> isize { *self ^ rhs } }
impl BitXor<&isize> for isize { type Output = isize; fn bitxor(self, rhs: &isize) -> isize { self ^ *rhs } }
impl BitXor<&isize> for &isize { type Output = isize; fn bitxor(self, rhs: &isize) -> isize { *self ^ *rhs } }
impl BitXorAssign<&isize> for isize { fn bitxor_assign(&mut self, rhs: &isize) { *self ^= *rhs; } }
impl Neg for &isize { type Output = isize; fn neg(self) -> isize { -*self } }
impl Not for &isize { type Output = isize; fn not(self) -> isize { !*self } }
impl Add<u8> for &u8 { type Output = u8; fn add(self, rhs: u8) -> u8 { *self + rhs } }
impl Add<&u8> for u8 { type Output = u8; fn add(self, rhs: &u8) -> u8 { self + *rhs } }
impl Add<&u8> for &u8 { type Output = u8; fn add(self, rhs: &u8) -> u8 { *self + *rhs } }
impl AddAssign<&u8> for u8 { fn add_assign(&mut self, rhs: &u8) { *self += *rhs; } }
impl Sub<u8> for &u8 { type Output = u8; fn sub(self, rhs: u8) -> u8 { *self - rhs } }
impl Sub<&u8> for u8 { type Output = u8; fn sub(self, rhs: &u8) -> u8 { self - *rhs } }
impl Sub<&u8> for &u8 { type Output = u8; fn sub(self, rhs: &u8) -> u8 { *self - *rhs } }
impl SubAssign<&u8> for u8 { fn sub_assign(&mut self, rhs: &u8) { *self -= *rhs; } }
impl Mul<u8> for &u8 { type Output = u8; fn mul(self, rhs: u8) -> u8 { *self * rhs } }
impl Mul<&u8> for u8 { type Output = u8; fn mul(self, rhs: &u8) -> u8 { self * *rhs } }
impl Mul<&u8> for &u8 { type Output = u8; fn mul(self, rhs: &u8) -> u8 { *self * *rhs } }
impl MulAssign<&u8> for u8 { fn mul_assign(&mut self, rhs: &u8) { *self *= *rhs; } }
impl Div<u8> for &u8 { type Output = u8; fn div(self, rhs: u8) -> u8 { *self / rhs } }
impl Div<&u8> for u8 { type Output = u8; fn div(self, rhs: &u8) -> u8 { self / *rhs } }
impl Div<&u8> for &u8 { type Output = u8; fn div(self, rhs: &u8) -> u8 { *self / *rhs } }
impl DivAssign<&u8> for u8 { fn div_assign(&mut self, rhs: &u8) { *self /= *rhs; } }
impl Rem<u8> for &u8 { type Output = u8; fn rem(self, rhs: u8) -> u8 { *self % rhs } }
impl Rem<&u8> for u8 { type Output = u8; fn rem(self, rhs: &u8) -> u8 { self % *rhs } }
impl Rem<&u8> for &u8 { type Output = u8; fn rem(self, rhs: &u8) -> u8 { *self % *rhs } }
impl RemAssign<&u8> for u8 { fn rem_assign(&mut self, rhs: &u8) { *self %= *rhs; } }
impl BitAnd<u8> for &u8 { type Output = u8; fn bitand(self, rhs: u8) -> u8 { *self & rhs } }
impl BitAnd<&u8> for u8 { type Output = u8; fn bitand(self, rhs: &u8) -> u8 { self & *rhs } }
impl BitAnd<&u8> for &u8 { type Output = u8; fn bitand(self, rhs: &u8) -> u8 { *self & *rhs } }
impl BitAndAssign<&u8> for u8 { fn bitand_assign(&mut self, rhs: &u8) { *self &= *rhs; } }
impl BitOr<u8> for &u8 { type Output = u8; fn bitor(self, rhs: u8) -> u8 { *self | rhs } }
impl BitOr<&u8> for u8 { type Output = u8; fn bitor(self, rhs: &u8) -> u8 { self | *rhs } }
impl BitOr<&u8> for &u8 { type Output = u8; fn bitor(self, rhs: &u8) -> u8 { *self | *rhs } }
impl BitOrAssign<&u8> for u8 { fn bitor_assign(&mut self, rhs: &u8) { *self |= *rhs; } }
impl BitXor<u8> for &u8 { type Output = u8; fn bitxor(self, rhs: u8) -> u8 { *self ^ rhs } }
impl BitXor<&u8> for u8 { type Output = u8; fn bitxor(self, rhs: &u8) -> u8 { self ^ *rhs } }
impl BitXor<&u8> for &u8 { type Output = u8; fn bitxor(self, rhs: &u8) -> u8 { *self ^ *rhs } }
impl BitXorAssign<&u8> for u8 { fn bitxor_assign(&mut self, rhs: &u8) { *self ^= *rhs; } }
impl Not for &u8 { type Output = u8; fn not(self) -> u8 { !*self } }
impl Add<u16> for &u16 { type Output = u16; fn add(self, rhs: u16) -> u16 { *self + rhs } }
impl Add<&u16> for u16 { type Output = u16; fn add(self, rhs: &u16) -> u16 { self + *rhs } }
impl Add<&u16> for &u16 { type Output = u16; fn add(self, rhs: &u16) -> u16 { *self + *rhs } }
impl AddAssign<&u16> for u16 { fn add_assign(&mut self, rhs: &u16) { *self += *rhs; } }
impl Sub<u16> for &u16 { type Output = u16; fn sub(self, rhs: u16) -> u16 { *self - rhs } }
impl Sub<&u16> for u16 { type Output = u16; fn sub(self, rhs: &u16) -> u16 { self - *rhs } }
impl Sub<&u16> for &u16 { type Output = u16; fn sub(self, rhs: &u16) -> u16 { *self - *rhs } }
impl SubAssign<&u16> for u16 { fn sub_assign(&mut self, rhs: &u16) { *self -= *rhs; } }
impl Mul<u16> for &u16 { type Output = u16; fn mul(self, rhs: u16) -> u16 { *self * rhs } }
impl Mul<&u16> for u16 { type Output = u16; fn mul(self, rhs: &u16) -> u16 { self * *rhs } }
impl Mul<&u16> for &u16 { type Output = u16; fn mul(self, rhs: &u16) -> u16 { *self * *rhs } }
impl MulAssign<&u16> for u16 { fn mul_assign(&mut self, rhs: &u16) { *self *= *rhs; } }
impl Div<u16> for &u16 { type Output = u16; fn div(self, rhs: u16) -> u16 { *self / rhs } }
impl Div<&u16> for u16 { type Output = u16; fn div(self, rhs: &u16) -> u16 { self / *rhs } }
impl Div<&u16> for &u16 { type Output = u16; fn div(self, rhs: &u16) -> u16 { *self / *rhs } }
impl DivAssign<&u16> for u16 { fn div_assign(&mut self, rhs: &u16) { *self /= *rhs; } }
impl Rem<u16> for &u16 { type Output = u16; fn rem(self, rhs: u16) -> u16 { *self % rhs } }
impl Rem<&u16> for u16 { type Output = u16; fn rem(self, rhs: &u16) -> u16 { self % *rhs } }
impl Rem<&u16> for &u16 { type Output = u16; fn rem(self, rhs: &u16) -> u16 { *self % *rhs } }
impl RemAssign<&u16> for u16 { fn rem_assign(&mut self, rhs: &u16) { *self %= *rhs; } }
impl BitAnd<u16> for &u16 { type Output = u16; fn bitand(self, rhs: u16) -> u16 { *self & rhs } }
impl BitAnd<&u16> for u16 { type Output = u16; fn bitand(self, rhs: &u16) -> u16 { self & *rhs } }
impl BitAnd<&u16> for &u16 { type Output = u16; fn bitand(self, rhs: &u16) -> u16 { *self & *rhs } }
impl BitAndAssign<&u16> for u16 { fn bitand_assign(&mut self, rhs: &u16) { *self &= *rhs; } }
impl BitOr<u16> for &u16 { type Output = u16; fn bitor(self, rhs: u16) -> u16 { *self | rhs } }
impl BitOr<&u16> for u16 { type Output = u16; fn bitor(self, rhs: &u16) -> u16 { self | *rhs } }
impl BitOr<&u16> for &u16 { type Output = u16; fn bitor(self, rhs: &u16) -> u16 { *self | *rhs } }
impl BitOrAssign<&u16> for u16 { fn bitor_assign(&mut self, rhs: &u16) { *self |= *rhs; } }
impl BitXor<u16> for &u16 { type Output = u16; fn bitxor(self, rhs: u16) -> u16 { *self ^ rhs } }
impl BitXor<&u16> for u16 { type Output = u16; fn bitxor(self, rhs: &u16) -> u16 { self ^ *rhs } }
impl BitXor<&u16> for &u16 { type Output = u16; fn bitxor(self, rhs: &u16) -> u16 { *self ^ *rhs } }
impl BitXorAssign<&u16> for u16 { fn bitxor_assign(&mut self, rhs: &u16) { *self ^= *rhs; } }
impl Not for &u16 { type Output = u16; fn not(self) -> u16 { !*self } }
impl Add<u32> for &u32 { type Output = u32; fn add(self, rhs: u32) -> u32 { *self + rhs } }
impl Add<&u32> for u32 { type Output = u32; fn add(self, rhs: &u32) -> u32 { self + *rhs } }
impl Add<&u32> for &u32 { type Output = u32; fn add(self, rhs: &u32) -> u32 { *self + *rhs } }
impl AddAssign<&u32> for u32 { fn add_assign(&mut self, rhs: &u32) { *self += *rhs; } }
impl Sub<u32> for &u32 { type Output = u32; fn sub(self, rhs: u32) -> u32 { *self - rhs } }
impl Sub<&u32> for u32 { type Output = u32; fn sub(self, rhs: &u32) -> u32 { self - *rhs } }
impl Sub<&u32> for &u32 { type Output = u32; fn sub(self, rhs: &u32) -> u32 { *self - *rhs } }
impl SubAssign<&u32> for u32 { fn sub_assign(&mut self, rhs: &u32) { *self -= *rhs; } }
impl Mul<u32> for &u32 { type Output = u32; fn mul(self, rhs: u32) -> u32 { *self * rhs } }
impl Mul<&u32> for u32 { type Output = u32; fn mul(self, rhs: &u32) -> u32 { self * *rhs } }
impl Mul<&u32> for &u32 { type Output = u32; fn mul(self, rhs: &u32) -> u32 { *self * *rhs } }
impl MulAssign<&u32> for u32 { fn mul_assign(&mut self, rhs: &u32) { *self *= *rhs; } }
impl Div<u32> for &u32 { type Output = u32; fn div(self, rhs: u32) -> u32 { *self / rhs } }
impl Div<&u32> for u32 { type Output = u32; fn div(self, rhs: &u32) -> u32 { self / *rhs } }
impl Div<&u32> for &u32 { type Output = u32; fn div(self, rhs: &u32) -> u32 { *self / *rhs } }
impl DivAssign<&u32> for u32 { fn div_assign(&mut self, rhs: &u32) { *self /= *rhs; } }
impl Rem<u32> for &u32 { type Output = u32; fn rem(self, rhs: u32) -> u32 { *self % rhs } }
impl Rem<&u32> for u32 { type Output = u32; fn rem(self, rhs: &u32) -> u32 { self % *rhs } }
impl Rem<&u32> for &u32 { type Output = u32; fn rem(self, rhs: &u32) -> u32 { *self % *rhs } }
impl RemAssign<&u32> for u32 { fn rem_assign(&mut self, rhs: &u32) { *self %= *rhs; } }
impl BitAnd<u32> for &u32 { type Output = u32; fn bitand(self, rhs: u32) -> u32 { *self & rhs } }
impl BitAnd<&u32> for u32 { type Output = u32; fn bitand(self, rhs: &u32) -> u32 { self & *rhs } }
impl BitAnd<&u32> for &u32 { type Output = u32; fn bitand(self, rhs: &u32) -> u32 { *self & *rhs } }
impl BitAndAssign<&u32> for u32 { fn bitand_assign(&mut self, rhs: &u32) { *self &= *rhs; } }
impl BitOr<u32> for &u32 { type Output = u32; fn bitor(self, rhs: u32) -> u32 { *self | rhs } }
impl BitOr<&u32> for u32 { type Output = u32; fn bitor(self, rhs: &u32) -> u32 { self | *rhs } }
impl BitOr<&u32> for &u32 { type Output = u32; fn bitor(self, rhs: &u32) -> u32 { *self | *rhs } }
impl BitOrAssign<&u32> for u32 { fn bitor_assign(&mut self, rhs: &u32) { *self |= *rhs; } }
impl BitXor<u32> for &u32 { type Output = u32; fn bitxor(self, rhs: u32) -> u32 { *self ^ rhs } }
impl BitXor<&u32> for u32 { type Output = u32; fn bitxor(self, rhs: &u32) -> u32 { self ^ *rhs } }
impl BitXor<&u32> for &u32 { type Output = u32; fn bitxor(self, rhs: &u32) -> u32 { *self ^ *rhs } }
impl BitXorAssign<&u32> for u32 { fn bitxor_assign(&mut self, rhs: &u32) { *self ^= *rhs; } }
impl Not for &u32 { type Output = u32; fn not(self) -> u32 { !*self } }
impl Add<u64> for &u64 { type Output = u64; fn add(self, rhs: u64) -> u64 { *self + rhs } }
impl Add<&u64> for u64 { type Output = u64; fn add(self, rhs: &u64) -> u64 { self + *rhs } }
impl Add<&u64> for &u64 { type Output = u64; fn add(self, rhs: &u64) -> u64 { *self + *rhs } }
impl AddAssign<&u64> for u64 { fn add_assign(&mut self, rhs: &u64) { *self += *rhs; } }
impl Sub<u64> for &u64 { type Output = u64; fn sub(self, rhs: u64) -> u64 { *self - rhs } }
impl Sub<&u64> for u64 { type Output = u64; fn sub(self, rhs: &u64) -> u64 { self - *rhs } }
impl Sub<&u64> for &u64 { type Output = u64; fn sub(self, rhs: &u64) -> u64 { *self - *rhs } }
impl SubAssign<&u64> for u64 { fn sub_assign(&mut self, rhs: &u64) { *self -= *rhs; } }
impl Mul<u64> for &u64 { type Output = u64; fn mul(self, rhs: u64) -> u64 { *self * rhs } }
impl Mul<&u64> for u64 { type Output = u64; fn mul(self, rhs: &u64) -> u64 { self * *rhs } }
impl Mul<&u64> for &u64 { type Output = u64; fn mul(self, rhs: &u64) -> u64 { *self * *rhs } }
impl MulAssign<&u64> for u64 { fn mul_assign(&mut self, rhs: &u64) { *self *= *rhs; } }
impl Div<u64> for &u64 { type Output = u64; fn div(self, rhs: u64) -> u64 { *self / rhs } }
impl Div<&u64> for u64 { type Output = u64; fn div(self, rhs: &u64) -> u64 { self / *rhs } }
impl Div<&u64> for &u64 { type Output = u64; fn div(self, rhs: &u64) -> u64 { *self / *rhs } }
impl DivAssign<&u64> for u64 { fn div_assign(&mut self, rhs: &u64) { *self /= *rhs; } }
impl Rem<u64> for &u64 { type Output = u64; fn rem(self, rhs: u64) -> u64 { *self % rhs } }
impl Rem<&u64> for u64 { type Output = u64; fn rem(self, rhs: &u64) -> u64 { self % *rhs } }
impl Rem<&u64> for &u64 { type Output = u64; fn rem(self, rhs: &u64) -> u64 { *self % *rhs } }
impl RemAssign<&u64> for u64 { fn rem_assign(&mut self, rhs: &u64) { *self %= *rhs; } }
impl BitAnd<u64> for &u64 { type Output = u64; fn bitand(self, rhs: u64) -> u64 { *self & rhs } }
impl BitAnd<&u64> for u64 { type Output = u64; fn bitand(self, rhs: &u64) -> u64 { self & *rhs } }
impl BitAnd<&u64> for &u64 { type Output = u64; fn bitand(self, rhs: &u64) -> u64 { *self & *rhs } }
impl BitAndAssign<&u64> for u64 { fn bitand_assign(&mut self, rhs: &u64) { *self &= *rhs; } }
impl BitOr<u64> for &u64 { type Output = u64; fn bitor(self, rhs: u64) -> u64 { *self | rhs } }
impl BitOr<&u64> for u64 { type Output = u64; fn bitor(self, rhs: &u64) -> u64 { self | *rhs } }
impl BitOr<&u64> for &u64 { type Output = u64; fn bitor(self, rhs: &u64) -> u64 { *self | *rhs } }
impl BitOrAssign<&u64> for u64 { fn bitor_assign(&mut self, rhs: &u64) { *self |= *rhs; } }
impl BitXor<u64> for &u64 { type Output = u64; fn bitxor(self, rhs: u64) -> u64 { *self ^ rhs } }
impl BitXor<&u64> for u64 { type Output = u64; fn bitxor(self, rhs: &u64) -> u64 { self ^ *rhs } }
impl BitXor<&u64> for &u64 { type Output = u64; fn bitxor(self, rhs: &u64) -> u64 { *self ^ *rhs } }
impl BitXorAssign<&u64> for u64 { fn bitxor_assign(&mut self, rhs: &u64) { *self ^= *rhs; } }
impl Not for &u64 { type Output = u64; fn not(self) -> u64 { !*self } }
impl Add<u128> for &u128 { type Output = u128; fn add(self, rhs: u128) -> u128 { *self + rhs } }
impl Add<&u128> for u128 { type Output = u128; fn add(self, rhs: &u128) -> u128 { self + *rhs } }
impl Add<&u128> for &u128 { type Output = u128; fn add(self, rhs: &u128) -> u128 { *self + *rhs } }
impl AddAssign<&u128> for u128 { fn add_assign(&mut self, rhs: &u128) { *self += *rhs; } }
impl Sub<u128> for &u128 { type Output = u128; fn sub(self, rhs: u128) -> u128 { *self - rhs } }
impl Sub<&u128> for u128 { type Output = u128; fn sub(self, rhs: &u128) -> u128 { self - *rhs } }
impl Sub<&u128> for &u128 { type Output = u128; fn sub(self, rhs: &u128) -> u128 { *self - *rhs } }
impl SubAssign<&u128> for u128 { fn sub_assign(&mut self, rhs: &u128) { *self -= *rhs; } }
impl Mul<u128> for &u128 { type Output = u128; fn mul(self, rhs: u128) -> u128 { *self * rhs } }
impl Mul<&u128> for u128 { type Output = u128; fn mul(self, rhs: &u128) -> u128 { self * *rhs } }
impl Mul<&u128> for &u128 { type Output = u128; fn mul(self, rhs: &u128) -> u128 { *self * *rhs } }
impl MulAssign<&u128> for u128 { fn mul_assign(&mut self, rhs: &u128) { *self *= *rhs; } }
impl Div<u128> for &u128 { type Output = u128; fn div(self, rhs: u128) -> u128 { *self / rhs } }
impl Div<&u128> for u128 { type Output = u128; fn div(self, rhs: &u128) -> u128 { self / *rhs } }
impl Div<&u128> for &u128 { type Output = u128; fn div(self, rhs: &u128) -> u128 { *self / *rhs } }
impl DivAssign<&u128> for u128 { fn div_assign(&mut self, rhs: &u128) { *self /= *rhs; } }
impl Rem<u128> for &u128 { type Output = u128; fn rem(self, rhs: u128) -> u128 { *self % rhs } }
impl Rem<&u128> for u128 { type Output = u128; fn rem(self, rhs: &u128) -> u128 { self % *rhs } }
impl Rem<&u128> for &u128 { type Output = u128; fn rem(self, rhs: &u128) -> u128 { *self % *rhs } }
impl RemAssign<&u128> for u128 { fn rem_assign(&mut self, rhs: &u128) { *self %= *rhs; } }
impl BitAnd<u128> for &u128 { type Output = u128; fn bitand(self, rhs: u128) -> u128 { *self & rhs } }
impl BitAnd<&u128> for u128 { type Output = u128; fn bitand(self, rhs: &u128) -> u128 { self & *rhs } }
impl BitAnd<&u128> for &u128 { type Output = u128; fn bitand(self, rhs: &u128) -> u128 { *self & *rhs } }
impl BitAndAssign<&u128> for u128 { fn bitand_assign(&mut self, rhs: &u128) { *self &= *rhs; } }
impl BitOr<u128> for &u128 { type Output = u128; fn bitor(self, rhs: u128) -> u128 { *self | rhs } }
impl BitOr<&u128> for u128 { type Output = u128; fn bitor(self, rhs: &u128) -> u128 { self | *rhs } }
impl BitOr<&u128> for &u128 { type Output = u128; fn bitor(self, rhs: &u128) -> u128 { *self | *rhs } }
impl BitOrAssign<&u128> for u128 { fn bitor_assign(&mut self, rhs: &u128) { *self |= *rhs; } }
impl BitXor<u128> for &u128 { type Output = u128; fn bitxor(self, rhs: u128) -> u128 { *self ^ rhs } }
impl BitXor<&u128> for u128 { type Output = u128; fn bitxor(self, rhs: &u128) -> u128 { self ^ *rhs } }
impl BitXor<&u128> for &u128 { type Output = u128; fn bitxor(self, rhs: &u128) -> u128 { *self ^ *rhs } }
impl BitXorAssign<&u128> for u128 { fn bitxor_assign(&mut self, rhs: &u128) { *self ^= *rhs; } }
impl Not for &u128 { type Output = u128; fn not(self) -> u128 { !*self } }
impl Add<usize> for &usize { type Output = usize; fn add(self, rhs: usize) -> usize { *self + rhs } }
impl Add<&usize> for usize { type Output = usize; fn add(self, rhs: &usize) -> usize { self + *rhs } }
impl Add<&usize> for &usize { type Output = usize; fn add(self, rhs: &usize) -> usize { *self + *rhs } }
impl AddAssign<&usize> for usize { fn add_assign(&mut self, rhs: &usize) { *self += *rhs; } }
impl Sub<usize> for &usize { type Output = usize; fn sub(self, rhs: usize) -> usize { *self - rhs } }
impl Sub<&usize> for usize { type Output = usize; fn sub(self, rhs: &usize) -> usize { self - *rhs } }
impl Sub<&usize> for &usize { type Output = usize; fn sub(self, rhs: &usize) -> usize { *self - *rhs } }
impl SubAssign<&usize> for usize { fn sub_assign(&mut self, rhs: &usize) { *self -= *rhs; } }
impl Mul<usize> for &usize { type Output = usize; fn mul(self, rhs: usize) -> usize { *self * rhs } }
impl Mul<&usize> for usize { type Output = usize; fn mul(self, rhs: &usize) -> usize { self * *rhs } }
impl Mul<&usize> for &usize { type Output = usize; fn mul(self, rhs: &usize) -> usize { *self * *rhs } }
impl MulAssign<&usize> for usize { fn mul_assign(&mut self, rhs: &usize) { *self *= *rhs; } }
impl Div<usize> for &usize { type Output = usize; fn div(self, rhs: usize) -> usize { *self / rhs } }
impl Div<&usize> for usize { type Output = usize; fn div(self, rhs: &usize) -> usize { self / *rhs } }
impl Div<&usize> for &usize { type Output = usize; fn div(self, rhs: &usize) -> usize { *self / *rhs } }
impl DivAssign<&usize> for usize { fn div_assign(&mut self, rhs: &usize) { *self /= *rhs; } }
impl Rem<usize> for &usize { type Output = usize; fn rem(self, rhs: usize) -> usize { *self % rhs } }
impl Rem<&usize> for usize { type Output = usize; fn rem(self, rhs: &usize) -> usize { self % *rhs } }
impl Rem<&usize> for &usize { type Output = usize; fn rem(self, rhs: &usize) -> usize { *self % *rhs } }
impl RemAssign<&usize> for usize { fn rem_assign(&mut self, rhs: &usize) { *self %= *rhs; } }
impl BitAnd<usize> for &usize { type Output = usize; fn bitand(self, rhs: usize) -> usize { *self & rhs } }
impl BitAnd<&usize> for usize { type Output = usize; fn bitand(self, rhs: &usize) -> usize { self & *rhs } }
impl BitAnd<&usize> for &usize { type Output = usize; fn bitand(self, rhs: &usize) -> usize { *self & *rhs } }
impl BitAndAssign<&usize> for usize { fn bitand_assign(&mut self, rhs: &usize) { *self &= *rhs; } }
impl BitOr<usize> for &usize { type Output = usize; fn bitor(self, rhs: usize) -> usize { *self | rhs } }
impl BitOr<&usize> for usize { type Output = usize; fn bitor(self, rhs: &usize) -> usize { self | *rhs } }
impl BitOr<&usize> for &usize { type Output = usize; fn bitor(self, rhs: &usize) -> usize { *self | *rhs } }
impl BitOrAssign<&usize> for usize { fn bitor_assign(&mut self, rhs: &usize) { *self |= *rhs; } }
impl BitXor<usize> for &usize { type Output = usize; fn bitxor(self, rhs: usize) -> usize { *self ^ rhs } }
impl BitXor<&usize> for usize { type Output = usize; fn bitxor(self, rhs: &usize) -> usize { self ^ *rhs } }
impl BitXor<&usize> for &usize { type Output = usize; fn bitxor(self, rhs: &usize) -> usize { *self ^ *rhs } }
impl BitXorAssign<&usize> for usize { fn bitxor_assign(&mut self, rhs: &usize) { *self ^= *rhs; } }
impl Not for &usize { type Output = usize; fn not(self) -> usize { !*self } }
impl BitAnd<bool> for &bool { type Output = bool; fn bitand(self, rhs: bool) -> bool { *self & rhs } }
impl BitAnd<&bool> for bool { type Output = bool; fn bitand(self, rhs: &bool) -> bool { self & *rhs } }
impl BitAnd<&bool> for &bool { type Output = bool; fn bitand(self, rhs: &bool) -> bool { *self & *rhs } }
impl BitAndAssign<&bool> for bool { fn bitand_assign(&mut self, rhs: &bool) { *self &= *rhs; } }
impl BitOr<bool> for &bool { type Output = bool; fn bitor(self, rhs: bool) -> bool { *self | rhs } }
impl BitOr<&bool> for bool { type Output = bool; fn bitor(self, rhs: &bool) -> bool { self | *rhs } }
impl BitOr<&bool> for &bool { type Output = bool; fn bitor(self, rhs: &bool) -> bool { *self | *rhs } }
impl BitOrAssign<&bool> for bool { fn bitor_assign(&mut self, rhs: &bool) { *self |= *rhs; } }
impl BitXor<bool> for &bool { type Output = bool; fn bitxor(self, rhs: bool) -> bool { *self ^ rhs } }
impl BitXor<&bool> for bool { type Output = bool; fn bitxor(self, rhs: &bool) -> bool { self ^ *rhs } }
impl BitXor<&bool> for &bool { type Output = bool; fn bitxor(self, rhs: &bool) -> bool { *self ^ *rhs } }
impl BitXorAssign<&bool> for bool { fn bitxor_assign(&mut self, rhs: &bool) { *self ^= *rhs; } }
impl Not for &bool { type Output = bool; fn not(self) -> bool { !*self } }
