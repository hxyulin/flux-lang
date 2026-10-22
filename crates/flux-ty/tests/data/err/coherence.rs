// Overlapping `impl` blocks (E0119), found by unifying their whole
// headers, and named as rustc names them: the trait with its arguments,
// but not those that are their defaults, and `_` for what either block
// leaves open. `core`'s `impl<T> From<T> for T` overlaps `From<S> for S`
// only; `From<X> for W<X>` would need `X = W<X>`.

use core::ops::Add;

struct S(i32);

struct W<X>(X);

impl Add for S {
    type Output = S;

    fn add(self, rhs: S) -> S {
        rhs
    }
}

impl Add<S> for S {
    type Output = S;

    fn add(self, rhs: S) -> S {
        rhs
    }
}

impl Add<i32> for S {
    type Output = S;

    fn add(self, _rhs: i32) -> S {
        self
    }
}

impl Add<i32> for S {
    type Output = S;

    fn add(self, _rhs: i32) -> S {
        self
    }
}

impl From<S> for S {
    fn from(s: S) -> S {
        s
    }
}

impl From<S> for i32 {
    fn from(s: S) -> i32 {
        s.0
    }
}

impl<X> From<X> for W<X> {
    fn from(x: X) -> W<X> {
        W(x)
    }
}

impl<X> From<X> for W<X> {
    fn from(x: X) -> W<X> {
        W(x)
    }
}

impl<X> From<W<X>> for W<X> {
    fn from(w: W<X>) -> W<X> {
        w
    }
}

fn main() {}
