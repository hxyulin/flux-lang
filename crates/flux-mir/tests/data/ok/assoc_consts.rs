// Associated constants in MIR: one of a trait or a generic block is
// named with its generic arguments, which monomorphization fills in.

trait Shape {
    const SIDES: u32;
    const DOUBLE: u32 = Self::SIDES * 2;
}

struct Square;

impl Shape for Square {
    const SIDES: u32 = 4;
}

struct Wrap<T>(T);

impl<T: Shape> Wrap<T> {
    const INNER: u32 = T::SIDES + 1;
}

fn sides<T: Shape>() -> u32 {
    T::SIDES + <T as Shape>::DOUBLE + Wrap::<T>::INNER
}

fn main() {
    let _ = sides::<Square>() + Square::DOUBLE;
}
