// Moves and borrows through overloaded operators: binary and unary
// operators move non-`Copy` operands, compound assignment needs a
// mutable place, and `==` borrows both operands.

use core::ops::{Add, AddAssign, Neg};

struct V(i32);

struct Pair(V, V);

impl Add for V {
    type Output = V;

    fn add(self, rhs: V) -> V {
        V(self.0 + rhs.0)
    }
}

impl Neg for V {
    type Output = V;

    fn neg(self) -> V {
        V(-self.0)
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

// Use after an operator moved the operand.
fn use_after_add(a: V, b: V) -> i32 {
    let _c = a + b;
    a.0
}

fn add_self(a: V) -> V {
    a + a
}

fn add_self_generic<T: Add<Output = T>>(a: T) -> T {
    a + a
}

fn use_after_neg(a: V) -> V {
    let _b = -a;
    a
}

fn compare_after_move(a: V, b: V) -> bool {
    let _c = a + V(1);
    a == b
}

fn compound_after_move(a: V, mut b: V) {
    let _c = -a;
    b += a;
}

fn add_in_loop(a: V) {
    loop {
        let _b = a + V(1);
    }
}

fn partial_move(p: Pair) -> Pair {
    let _c = p.0 + V(1);
    p
}

// Operators cannot move out of a reference.
fn add_behind_ref(r: &V) -> V {
    *r + V(1)
}

fn neg_behind_ref(r: &V) -> V {
    -*r
}

fn add_field_behind_ref(p: &Pair) -> V {
    p.0 + V(1)
}

// Compound assignment needs a mutable place, and borrows it before the
// right operand is moved.
fn compound_through_shared(r: &V) {
    *r += V(1);
}

fn compound_self(mut a: V) {
    a += a;
}

fn compound_moved_field(mut p: Pair) {
    p.0 += p.1;
    p.1 += p.0;
}

// Borrows held across an operator.
fn move_while_borrowed(a: V) -> i32 {
    let r = &a;
    let _c = a + V(1);
    r.0
}

fn compare_while_mutably_borrowed(mut a: V, b: V) -> i32 {
    let r = &mut a;
    let _e = a == b;
    r.0
}

fn compound_while_borrowed(mut a: V) -> i32 {
    let r = &a;
    a += V(1);
    r.0
}

// An operand that was never initialized.
fn uninitialized(b: V) -> V {
    let a: V;
    a + b
}

// An overloaded compound assignment borrows its place mutably: E0596
// without `mut`.
fn compound_immutable_arg(a: V) {
    a += V(1);
}

fn compound_immutable_local() {
    let b = V(1);
    b += V(1);
}

fn compound_immutable_generic<T: AddAssign>(a: T, c: T) {
    a += c;
}

fn main() {}
