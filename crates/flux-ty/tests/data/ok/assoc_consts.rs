// Associated constants: required and defaulted in traits, defined in
// their `impl` blocks and in generic inherent blocks, named through a
// type, a type parameter, `Self` and qualified paths, with the trait's
// arguments when it has some; their types follow the instance.

trait Shape {
    const SIDES: u32;
    const DOUBLE: u32 = Self::SIDES * 2;
    const PAIR: (u32, bool) = (Self::SIDES, true);

    fn sides(&self) -> u32 {
        Self::SIDES + Self::DOUBLE
    }
}

// A constant's type may be an associated type of its trait.
trait Sized2 {
    type Repr;
    const SIZE: Self::Repr;
}

impl Sized2 for Square {
    type Repr = u16;
    const SIZE: Self::Repr = 4;
}

trait Unit<T> {
    const ONE: T;
}

struct Square;

impl Shape for Square {
    const SIDES: u32 = 4;
}

impl Unit<u8> for Square {
    const ONE: u8 = 1;
}

impl Unit<i64> for Square {
    const ONE: i64 = 1;
}

struct Wrap<T>(T);

impl<T: Shape> Wrap<T> {
    const SIDES: u32 = T::SIDES;

    fn sides(&self) -> u32 {
        Self::SIDES
    }
}

impl<T: Shape> Shape for Wrap<T> {
    const SIDES: u32 = T::SIDES + 1;
    const DOUBLE: u32 = <T as Shape>::DOUBLE;
}

const EIGHT: u32 = Square::DOUBLE;

fn sides<T: Shape>() -> u32 {
    T::SIDES
}

fn main() {
    let a = Square::SIDES + <Square as Shape>::DOUBLE + EIGHT;
    let b: (u32, bool) = Square::PAIR;
    let c = sides::<Wrap<Square>>() + Wrap::<Square>::SIDES + Wrap(Square).sides();
    let one: u8 = <Square as Unit<u8>>::ONE;
    let wide: i64 = <Square as Unit<i64>>::ONE;
    let size: u16 = Square::SIZE;
    let _ = (a, b, c, one, wide, size, Square.sides());
}
