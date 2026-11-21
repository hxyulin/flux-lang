use std::ops::Mul;

trait Scalar: Mul<Self, Output = Self> + Sized {}
impl Scalar for u32 {}

trait VectorSpace: Mul<Self::Factor, Output = Self> + Sized {
    type Factor: Scalar;
}

impl<T: Scalar> VectorSpace for T {
    type Factor = T;
}

fn scale<V: VectorSpace>(value: V, factor: V::Factor) -> V {
    value * factor
}

fn main() {
    println!("{}", scale(21u32, 2));
}
