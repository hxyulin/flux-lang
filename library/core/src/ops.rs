//! Overloadable operators. On integers and `bool` the operators are
//! built in; on other types, `a + b` is `Add::add(a, b)`, `-a` is
//! `Neg::neg(a)`, `a += b` is `AddAssign::add_assign(&mut a, b)`, and so
//! on, through these traits, which the compiler knows by their `lang`
//! names. The integer types and `bool` implement them too, so that
//! generic code can use them; the impls are written out for each type,
//! since flux has no macros.
//!
//! `Drop` is here too, as in Rust: the code that runs when a value goes
//! out of scope.

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
impl Shl for i8 { type Output = i8; #[rustc_inherit_overflow_checks] fn shl(self, rhs: i8) -> i8 { self << rhs } }
impl ShlAssign for i8 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: i8) { *self <<= rhs; } }
impl Shr for i8 { type Output = i8; #[rustc_inherit_overflow_checks] fn shr(self, rhs: i8) -> i8 { self >> rhs } }
impl ShrAssign for i8 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: i8) { *self >>= rhs; } }
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
impl Shl for i16 { type Output = i16; #[rustc_inherit_overflow_checks] fn shl(self, rhs: i16) -> i16 { self << rhs } }
impl ShlAssign for i16 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: i16) { *self <<= rhs; } }
impl Shr for i16 { type Output = i16; #[rustc_inherit_overflow_checks] fn shr(self, rhs: i16) -> i16 { self >> rhs } }
impl ShrAssign for i16 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: i16) { *self >>= rhs; } }
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
impl Shl for i32 { type Output = i32; #[rustc_inherit_overflow_checks] fn shl(self, rhs: i32) -> i32 { self << rhs } }
impl ShlAssign for i32 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: i32) { *self <<= rhs; } }
impl Shr for i32 { type Output = i32; #[rustc_inherit_overflow_checks] fn shr(self, rhs: i32) -> i32 { self >> rhs } }
impl ShrAssign for i32 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: i32) { *self >>= rhs; } }
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
impl Shl for i64 { type Output = i64; #[rustc_inherit_overflow_checks] fn shl(self, rhs: i64) -> i64 { self << rhs } }
impl ShlAssign for i64 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: i64) { *self <<= rhs; } }
impl Shr for i64 { type Output = i64; #[rustc_inherit_overflow_checks] fn shr(self, rhs: i64) -> i64 { self >> rhs } }
impl ShrAssign for i64 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: i64) { *self >>= rhs; } }
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
impl Shl for i128 { type Output = i128; #[rustc_inherit_overflow_checks] fn shl(self, rhs: i128) -> i128 { self << rhs } }
impl ShlAssign for i128 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: i128) { *self <<= rhs; } }
impl Shr for i128 { type Output = i128; #[rustc_inherit_overflow_checks] fn shr(self, rhs: i128) -> i128 { self >> rhs } }
impl ShrAssign for i128 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: i128) { *self >>= rhs; } }
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
impl Shl for isize { type Output = isize; #[rustc_inherit_overflow_checks] fn shl(self, rhs: isize) -> isize { self << rhs } }
impl ShlAssign for isize { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: isize) { *self <<= rhs; } }
impl Shr for isize { type Output = isize; #[rustc_inherit_overflow_checks] fn shr(self, rhs: isize) -> isize { self >> rhs } }
impl ShrAssign for isize { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: isize) { *self >>= rhs; } }
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
impl Shl for u8 { type Output = u8; #[rustc_inherit_overflow_checks] fn shl(self, rhs: u8) -> u8 { self << rhs } }
impl ShlAssign for u8 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: u8) { *self <<= rhs; } }
impl Shr for u8 { type Output = u8; #[rustc_inherit_overflow_checks] fn shr(self, rhs: u8) -> u8 { self >> rhs } }
impl ShrAssign for u8 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: u8) { *self >>= rhs; } }
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
impl Shl for u16 { type Output = u16; #[rustc_inherit_overflow_checks] fn shl(self, rhs: u16) -> u16 { self << rhs } }
impl ShlAssign for u16 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: u16) { *self <<= rhs; } }
impl Shr for u16 { type Output = u16; #[rustc_inherit_overflow_checks] fn shr(self, rhs: u16) -> u16 { self >> rhs } }
impl ShrAssign for u16 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: u16) { *self >>= rhs; } }
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
impl Shl for u32 { type Output = u32; #[rustc_inherit_overflow_checks] fn shl(self, rhs: u32) -> u32 { self << rhs } }
impl ShlAssign for u32 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: u32) { *self <<= rhs; } }
impl Shr for u32 { type Output = u32; #[rustc_inherit_overflow_checks] fn shr(self, rhs: u32) -> u32 { self >> rhs } }
impl ShrAssign for u32 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: u32) { *self >>= rhs; } }
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
impl Shl for u64 { type Output = u64; #[rustc_inherit_overflow_checks] fn shl(self, rhs: u64) -> u64 { self << rhs } }
impl ShlAssign for u64 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: u64) { *self <<= rhs; } }
impl Shr for u64 { type Output = u64; #[rustc_inherit_overflow_checks] fn shr(self, rhs: u64) -> u64 { self >> rhs } }
impl ShrAssign for u64 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: u64) { *self >>= rhs; } }
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
impl Shl for u128 { type Output = u128; #[rustc_inherit_overflow_checks] fn shl(self, rhs: u128) -> u128 { self << rhs } }
impl ShlAssign for u128 { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: u128) { *self <<= rhs; } }
impl Shr for u128 { type Output = u128; #[rustc_inherit_overflow_checks] fn shr(self, rhs: u128) -> u128 { self >> rhs } }
impl ShrAssign for u128 { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: u128) { *self >>= rhs; } }
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
impl Shl for usize { type Output = usize; #[rustc_inherit_overflow_checks] fn shl(self, rhs: usize) -> usize { self << rhs } }
impl ShlAssign for usize { #[rustc_inherit_overflow_checks] fn shl_assign(&mut self, rhs: usize) { *self <<= rhs; } }
impl Shr for usize { type Output = usize; #[rustc_inherit_overflow_checks] fn shr(self, rhs: usize) -> usize { self >> rhs } }
impl ShrAssign for usize { #[rustc_inherit_overflow_checks] fn shr_assign(&mut self, rhs: usize) { *self >>= rhs; } }
impl Not for usize { type Output = usize; fn not(self) -> usize { !self } }
impl BitAnd for bool { type Output = bool; fn bitand(self, rhs: bool) -> bool { self & rhs } }
impl BitAndAssign for bool { fn bitand_assign(&mut self, rhs: bool) { *self &= rhs; } }
impl BitOr for bool { type Output = bool; fn bitor(self, rhs: bool) -> bool { self | rhs } }
impl BitOrAssign for bool { fn bitor_assign(&mut self, rhs: bool) { *self |= rhs; } }
impl BitXor for bool { type Output = bool; fn bitxor(self, rhs: bool) -> bool { self ^ rhs } }
impl BitXorAssign for bool { fn bitxor_assign(&mut self, rhs: bool) { *self ^= rhs; } }
impl Not for bool { type Output = bool; fn not(self) -> bool { !self } }
