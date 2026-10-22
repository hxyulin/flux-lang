// Errors involving overloaded operators: operators on types without the
// trait's impl, operands that do not match the selected impl, outputs used
// as another type, operators in generic code without the bound that
// allows them, and impls of the operator traits that do not match them.

use core::ops::{Add, Neg, Not, Sub};

struct S(i32);

struct V(i32);

struct N(i32);

impl Add for V {
    type Output = V;

    fn add(self, rhs: V) -> V {
        V(self.0 + rhs.0)
    }
}

impl Sub<i32> for V {
    type Output = i32;

    fn sub(self, rhs: i32) -> i32 {
        self.0 - rhs
    }
}

impl PartialEq for V {
    fn eq(&self, other: &V) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<i32> for N {
    fn eq(&self, other: &i32) -> bool {
        self.0 == *other
    }
}

// E0369: each binary operator on a type without its trait.
fn binary_without_impl() {
    let _a = S(1) + S(2);
    let _b = S(1) - S(2);
    let _c = S(1) * S(2);
    let _d = S(1) / S(2);
    let _e = S(1) % S(2);
    let _f = S(1) & S(2);
    let _g = S(1) | S(2);
    let _h = S(1) ^ S(2);
    let _i = S(1) << S(2);
    let _j = S(1) >> S(2);
    let _k = S(1) == S(2);
    let _l = S(1) != S(2);
}

// E0368: each compound assignment on a type without its trait, also when
// the binary operator's trait is implemented.
fn compound_without_impl(mut s: S, mut v: V) {
    s += S(2);
    s -= S(2);
    s *= S(2);
    s /= S(2);
    s %= S(2);
    s &= S(2);
    s |= S(2);
    s ^= S(2);
    s <<= S(2);
    s >>= S(2);
    v += V(1);
}

// E0600: unary operators on a type without the trait.
fn unary_without_impl() {
    let _a = -S(1);
    let _b = !S(1);
    let _c = -V(1);
}

// E0369 for operators on references when only the type has the impl.
fn references(a: &V, b: &V) {
    let _c = a + b;
    let _d = &V(1) + &V(2);
}

// E0308: `==` wants the same type on both sides, not a reference.
fn value_and_reference(a: V, b: V) {
    let _e = a == &b;
}

// E0308 at the right operand: the impl wants another type.
fn wrong_rhs() {
    let _a = V(1) + 1;
    let _b = V(1) - 1u8;
    let _c = V(1) - V(2);
    let _d = V(1) == 1;
}

// The comparison exists only from `N` to `i32`.
fn one_direction() {
    let _a = N(1) == N(2);
    let _b = 1 == N(1);
    let _c = N(1) == 1u8;
}

// E0277: an integer on the left of a struct.
fn integer_on_left() {
    let _a = 1 + V(1);
    let _b = 1 == V(1);
    let _c = 1u8 - V(1);
}

// E0308: the output is the impl's `Output`.
fn output_mismatch() {
    let _a: V = V(1) - 2;
    let _b: i32 = V(1) + V(2);
    let _c: V = V(1) == V(2);
}

// Generic code needs a bound for each operator, and the output is rigid.
fn without_bound<T>(a: T, b: T) {
    let _c = a + b;
}

fn unary_without_bound<T>(x: T, y: T) {
    let _a = -x;
    let _b = !y;
}

fn eq_without_bound<T>(a: &T, b: &T) -> bool {
    a == b
}

fn compound_without_bound<T>(mut a: T, b: T) {
    a += b;
}

fn add_bound_not_add_assign<T: Add<Output = T>>(mut a: T, b: T) {
    a += b;
}

fn output_is_rigid<T: Add>(a: T, b: T) -> T {
    a + b
}

fn neg_output_is_rigid<T: Neg>(x: T) -> T {
    -x
}

fn rhs_is_self<T: Add<Output = T>>(a: T) -> T {
    a + 1
}

fn other_bound<T: Not<Output = T>>(a: T, b: T) -> T {
    a - b
}

fn projection_without_bound<T: Tr>(x: T) {
    let _y = x.get() + 1;
}

trait Tr {
    type Out;

    fn get(&self) -> Self::Out;
}

// Impls of operator traits must match them.
struct A(i32);

impl Add for A {
    type Output = A;

    fn add(self, rhs: i32) -> A {
        A(self.0 + rhs)
    }
}

struct B(i32);

impl Add for B {
    fn add(self, rhs: B) -> B {
        B(self.0 + rhs.0)
    }
}

struct C(i32);

impl PartialEq for C {
    fn eq(&self, other: C) -> bool {
        self.0 == other.0
    }
}

// Built-in operators on mismatched integer types.
fn mixed_integers() {
    let _a = 1i32 + 1u8;
    let _b = -1u32;
}

// `core` compares references with references only: E0277 for a
// reference and a value, E0369 if the referent has no `PartialEq` at
// all, and E0277 for the referents when they cannot be compared.
fn reference_and_value(b: C, r: &C, i: &i32, flag: &bool) {
    let _a = r == b;
    let _b = r != b;
    let _c = i == 5;
    let _d = flag == true;
    let _e = 5 == i;
}

fn referents(r: &C, n: &N, m: &N, s: &S, t: &S, i: &i32) {
    let _a = s == t;
    let _b = n == m;
    let _c = r == i;
}

fn main() {}
