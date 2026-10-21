// Overloaded operators: on types other than integers and `bool`, every
// binary, compound assignment and unary operator, `==` and `!=` call the
// methods of the `core::ops` and `core::cmp` traits; the result is the
// impl's `Output`. Generic code uses them through bounds, and
// instantiating it with integers uses `core`'s impls.

use core::ops::{
    Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Div,
    DivAssign, Mul, MulAssign, Neg, Not, Rem, RemAssign, Shl, ShlAssign, Shr, ShrAssign, Sub,
    SubAssign,
};

struct V(i32, i32);

struct Bits(u8);

struct Flag(bool);

struct W<T>(T);

impl Add for V {
    type Output = V;

    fn add(self, rhs: V) -> V {
        V(self.0 + rhs.0, self.1 + rhs.1)
    }
}

impl Sub for V {
    type Output = V;

    fn sub(self, rhs: V) -> V {
        V(self.0 - rhs.0, self.1 - rhs.1)
    }
}

// A right operand of another type, and an output of another type.
impl Mul<i32> for V {
    type Output = V;

    fn mul(self, k: i32) -> V {
        V(self.0 * k, self.1 * k)
    }
}

impl Mul for V {
    type Output = i32;

    fn mul(self, rhs: V) -> i32 {
        self.0 * rhs.0 + self.1 * rhs.1
    }
}

impl Div<i32> for V {
    type Output = V;

    fn div(self, k: i32) -> V {
        V(self.0 / k, self.1 / k)
    }
}

impl Rem<i32> for V {
    type Output = V;

    fn rem(self, k: i32) -> V {
        V(self.0 % k, self.1 % k)
    }
}

impl Neg for V {
    type Output = V;

    fn neg(self) -> V {
        V(-self.0, -self.1)
    }
}

impl BitAnd for Bits {
    type Output = Bits;

    fn bitand(self, rhs: Bits) -> Bits {
        Bits(self.0 & rhs.0)
    }
}

impl BitOr for Bits {
    type Output = Bits;

    fn bitor(self, rhs: Bits) -> Bits {
        Bits(self.0 | rhs.0)
    }
}

impl BitXor for Bits {
    type Output = Bits;

    fn bitxor(self, rhs: Bits) -> Bits {
        Bits(self.0 ^ rhs.0)
    }
}

impl Shl<u32> for Bits {
    type Output = Bits;

    fn shl(self, n: u32) -> Bits {
        Bits(self.0 << n)
    }
}

impl Shr<u32> for Bits {
    type Output = Bits;

    fn shr(self, n: u32) -> Bits {
        Bits(self.0 >> n)
    }
}

impl Not for Bits {
    type Output = Bits;

    fn not(self) -> Bits {
        Bits(!self.0)
    }
}

impl Not for Flag {
    type Output = bool;

    fn not(self) -> bool {
        !self.0
    }
}

impl AddAssign for V {
    fn add_assign(&mut self, rhs: V) {
        self.0 += rhs.0;
        self.1 += rhs.1;
    }
}

impl SubAssign for V {
    fn sub_assign(&mut self, rhs: V) {
        self.0 -= rhs.0;
        self.1 -= rhs.1;
    }
}

impl MulAssign<i32> for V {
    fn mul_assign(&mut self, k: i32) {
        self.0 *= k;
        self.1 *= k;
    }
}

impl DivAssign<i32> for V {
    fn div_assign(&mut self, k: i32) {
        self.0 /= k;
        self.1 /= k;
    }
}

impl RemAssign<i32> for V {
    fn rem_assign(&mut self, k: i32) {
        self.0 %= k;
        self.1 %= k;
    }
}

impl BitAndAssign for Bits {
    fn bitand_assign(&mut self, rhs: Bits) {
        self.0 &= rhs.0;
    }
}

impl BitOrAssign for Bits {
    fn bitor_assign(&mut self, rhs: Bits) {
        self.0 |= rhs.0;
    }
}

impl BitXorAssign for Bits {
    fn bitxor_assign(&mut self, rhs: Bits) {
        self.0 ^= rhs.0;
    }
}

impl ShlAssign<u32> for Bits {
    fn shl_assign(&mut self, n: u32) {
        self.0 <<= n;
    }
}

impl ShrAssign<u32> for Bits {
    fn shr_assign(&mut self, n: u32) {
        self.0 >>= n;
    }
}

impl PartialEq for V {
    fn eq(&self, other: &V) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
}

impl Eq for V {}

// Comparison with another type, in one direction only.
impl PartialEq<i32> for Bits {
    fn eq(&self, other: &i32) -> bool {
        self.0 as i32 == *other
    }
}

// An impl for a generic type, through the parameter's impl.
impl<T: Add<Output = T>> Add for W<T> {
    type Output = W<T>;

    fn add(self, rhs: W<T>) -> W<T> {
        W(self.0 + rhs.0)
    }
}

impl<T: PartialEq> PartialEq for W<T> {
    fn eq(&self, other: &W<T>) -> bool {
        self.0 == other.0
    }
}

fn arithmetic() -> V {
    let a = V(1, 2) + V(3, 4);
    let b = a - V(1, 1);
    let c = b * 3 / 2 % 5;
    let d: i32 = V(1, 2) * V(3, 4);
    -c * d
}

fn chains() -> V {
    V(1, 1) + V(2, 2) + V(3, 3) - V(4, 4) * 2
}

fn bits() -> Bits {
    let a = Bits(12) & Bits(10);
    let b = a | Bits(1) ^ Bits(3);
    !(b << 2 >> 1)
}

fn unary() -> bool {
    let v: V = -V(1, 2);
    let b: bool = !Flag(false);
    b && v.0 < 0
}

fn compound() -> V {
    let mut v = V(1, 2);
    v += V(10, 20);
    v -= V(1, 1);
    v *= 3;
    v /= 2;
    v %= 7;
    v
}

fn compound_bits() -> Bits {
    let mut b = Bits(1);
    b |= Bits(6);
    b &= Bits(5);
    b ^= Bits(8);
    b <<= 2;
    b >>= 1;
    b
}

// Compound assignment through a reference and on a field.
fn compound_places(r: &mut V, w: &mut W<V>) {
    *r += V(1, 1);
    w.0 += V(2, 2);
}

fn equality() -> bool {
    let a = V(1, 2);
    let b = V(1, 2);
    let r = &a;
    a == b && !(a != b) && &a == &b && r == &b && Bits(3) == 3 && Bits(3) != 4
}

fn generic_struct() -> bool {
    let w = W(V(1, 2)) + W(V(3, 4));
    let n = W(5) + W(6);
    w == W(V(4, 6)) && n == W(11) && W(true) != W(false)
}

// Generic code through bounds.
fn sum3<T: Add<Output = T>>(a: T, b: T, c: T) -> T {
    a + b + c
}

fn add_output<T: Add>(a: T, b: T) -> T::Output {
    a + b
}

fn add_where<T, U>(a: T, b: U) -> <T as Add<U>>::Output
where
    T: Add<U>,
{
    a + b
}

fn negate<T: Neg<Output = T>>(x: T) -> T {
    -x
}

fn invert<T: Not>(x: T) -> T::Output {
    !x
}

fn accumulate<T: AddAssign>(mut acc: T, x: T, y: T) -> T {
    acc += x;
    acc += y;
    acc
}

fn same<T: PartialEq>(a: &T, b: &T) -> bool {
    a == b && !(a != b)
}

fn same_values<T: PartialEq>(a: T, b: T) -> bool {
    a == b
}

fn same_eq<T: Eq>(a: &T, b: &T) -> bool {
    a == b
}

fn mixed<T: PartialEq<i32>>(a: &T) -> bool {
    *a == 3
}

// Every operator trait through a bound, instantiated with integers and
// `bool`.
fn arith<T>(a: T, b: T, c: T, d: T, e: T) -> T
where
    T: Sub<Output = T> + Mul<Output = T> + Div<Output = T> + Rem<Output = T>,
{
    a - b * c / d % e
}

fn logic<T>(a: T, b: T, c: T, d: T) -> <T as Not>::Output
where
    T: BitAnd<Output = T> + BitOr<Output = T> + BitXor<Output = T> + Not,
{
    !(a & b | c ^ d)
}

fn shifts<T: Shl<u32, Output = T> + Shr<u32, Output = T>>(x: T) -> T {
    x << 3 >> 1
}

fn compound_generic<T>(mut x: T, a: T, b: T, c: T) -> T
where
    T: SubAssign + MulAssign + ShlAssign<T> + BitXorAssign,
{
    x -= a;
    x *= b;
    x <<= c;
    x
}

fn integer_impls() -> i64 {
    let a = arith(100i64, 3, 7, 2, 6) + arith(-50, 4, 3, 5, 7) as i64;
    let b = logic(12u8, 10, 5, 3) as i64 + logic(true, false, true, false) as i64 + logic(-3i16, 5, 9, 1) as i64;
    let c = shifts(5u32) as i64;
    let d = compound_generic(20i32, 2, 3, 1) as i64;
    a + b + c + d
}

fn generic_calls() -> i32 {
    let v = sum3(V(1, 1), V(2, 2), V(3, 3));
    let n = sum3(1, 2, 3) + sum3(10u8, 20, 30) as i32;
    let m: i64 = add_output(4i64, 5);
    let k: V = add_where(V(1, 1), V(2, 2));
    let neg = negate(V(1, 1)).0 + negate(7);
    let b = !invert(true) && !invert(Flag(true)) && invert(Bits(0)) == 255;
    let acc = accumulate(5u16, 6, 7);
    let eq = same(&V(1, 2), &V(1, 2)) && same(&5u64, &5) && same_values(true, true);
    let eq2 = same_eq(&V(1, 1), &V(1, 1)) && same_eq(&(), &()) && mixed(&Bits(3)) && mixed(&3);
    if b && eq && eq2 { v.0 + n + m as i32 + k.1 + neg + acc as i32 } else { 0 }
}

// Operators on associated types: normalized ones use the impl's type,
// and a bound on the projection or a binding allows them in generic code.
trait Tr {
    type Out;

    fn get(&self) -> Self::Out;
}

impl Tr for V {
    type Out = i32;

    fn get(&self) -> i32 {
        self.0
    }
}

impl Tr for Bits {
    type Out = V;

    fn get(&self) -> V {
        V(self.0 as i32, 0)
    }
}

fn add_gets<T: Tr>(a: &T) -> i32
where
    <T as Tr>::Out: Add<i32, Output = i32>,
{
    a.get() + 1
}

fn binding_ops<T: Tr<Out = i32>>(a: &T) -> i32 {
    a.get() * 2 + 1
}

fn projections() -> i32 {
    let a = V(1, 2).get() + V(3, 4).get();
    let b: V = Bits(1).get() + Bits(2).get();
    a + b.0 + binding_ops(&V(5, 6)) + add_gets(&V(7, 8))
}

fn main() {
    let _ = arithmetic();
    let _ = chains();
    let _ = bits();
    let _ = unary();
    let _ = compound();
    let _ = compound_bits();
    let mut v = V(0, 0);
    let mut w = W(V(0, 0));
    compound_places(&mut v, &mut w);
    let _ = equality();
    let _ = generic_struct();
    let _ = generic_calls();
    let _ = projections();
    let _ = integer_impls();
}
