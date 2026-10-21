// Overloaded operators in MIR: on types other than integers and `bool`
// they are calls of the trait's method. Binary and unary operators move
// their operands, compound assignment borrows the left operand mutably,
// and `==` and `!=` borrow both, so the operands stay usable; on
// references, they compare the pointees.

use core::ops::{Add, AddAssign, Neg, Not, Sub};

struct V(i32);

struct Pair(V, V);

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

impl Neg for V {
    type Output = V;

    fn neg(self) -> V {
        V(-self.0)
    }
}

impl Not for V {
    type Output = bool;

    fn not(self) -> bool {
        self.0 == 0
    }
}

impl AddAssign for V {
    fn add_assign(&mut self, rhs: V) {
        self.0 += rhs.0;
    }
}

impl PartialEq for V {
    fn eq(&self, other: &V) -> bool {
        self.0 == other.0
    }
}

// Both operands are moved into the call.
fn binary(a: V, b: V) -> V {
    a + b
}

// Temporaries, a right operand of another type, and chains.
fn temporaries() -> i32 {
    let v = V(1) + V(2) + V(3);
    v - 4
}

fn unary(a: V, b: V) -> bool {
    let n = -a;
    !b && !n
}

// The left operand is borrowed mutably, the right one moved.
fn compound(mut a: V, b: V) -> V {
    a += b;
    a += V(1);
    a
}

fn compound_places(r: &mut V, p: &mut Pair) {
    *r += V(1);
    p.0 += V(2);
}

// Moves out of fields.
fn fields(p: Pair) -> V {
    p.0 + p.1
}

// `==` and `!=` borrow both operands, which can be used afterwards.
fn equality(a: V, b: V) -> V {
    let e = a == b;
    let n = a != b;
    if e || n { a } else { b }
}

fn equality_temporaries() -> bool {
    V(1) == V(2)
}

// On references, `==` compares the pointees.
fn references(a: &V, b: &V) -> bool {
    a == b || &V(1) != a
}

// Integers and `bool` keep the built-in operators.
fn builtin(a: i32, b: bool) -> bool {
    let mut x = a + 1;
    x += 2;
    -x == a && !b
}

// Generic code calls the methods through the bounds.
fn generic_add<T: Add<Output = T>>(a: T, b: T, c: T) -> T {
    a + b + c
}

fn generic_compound<T: AddAssign>(mut a: T, b: T) -> T {
    a += b;
    a
}

fn generic_neg<T: Neg>(x: T) -> T::Output {
    -x
}

fn generic_eq<T: PartialEq>(a: &T, b: T) -> bool {
    *a == b && a != &b
}

fn main() {
    let _ = binary(V(1), V(2));
    let _ = temporaries();
    let _ = unary(V(1), V(0));
    let _ = compound(V(1), V(2));
    let mut v = V(0);
    let mut p = Pair(V(1), V(2));
    compound_places(&mut v, &mut p);
    let _ = fields(p);
    let _ = equality(V(1), V(2));
    let _ = equality_temporaries();
    let _ = references(&V(1), &V(1));
    let _ = builtin(1, true);
    let _ = generic_add(1, 2, 3);
    let _ = generic_add(V(1), V(2), V(3));
    let _ = generic_compound(5u8, 6);
    let _ = generic_neg(V(4));
    let _ = generic_eq(&V(1), V(1));
    let _ = generic_eq(&true, false);
}
