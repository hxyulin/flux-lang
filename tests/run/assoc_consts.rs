//! Associated constants: in traits, with or without a default, defined
//! in their `impl` blocks, and in generic `impl` blocks; used through a
//! type (`S::N`), a type parameter (`T::N`), `Self::N`, `<T as Tr>::N`,
//! and from other constants and `const fn`s, with a value per type.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn say(n: i32) {
    unsafe {
        printf(c"%d\n".as_ptr(), n);
    }
}

trait Shape {
    const SIDES: i32;
    const DOUBLE: i32 = Self::SIDES * 2;
    const NAME: (i32, bool) = (Self::SIDES, Self::SIDES > 3);

    fn describe(&self) -> i32 {
        Self::SIDES * 100 + Self::DOUBLE
    }
}

struct Triangle;
struct Square;

impl Shape for Triangle {
    const SIDES: i32 = 3;
}

impl Shape for Square {
    const SIDES: i32 = 4;
    const DOUBLE: i32 = 80;
}

struct Wrap<T>(T);

impl<T: Shape> Wrap<T> {
    const INNER: i32 = T::SIDES + 1000;

    fn inner(&self) -> i32 {
        Self::INNER
    }
}

impl<T: Shape> Shape for Wrap<T> {
    const SIDES: i32 = T::SIDES + 10;
}

trait Scaled<T> {
    const SCALE: i32;
}

impl Scaled<u8> for Triangle {
    const SCALE: i32 = 7;
}

impl Scaled<u16> for Triangle {
    const SCALE: i32 = 9;
}

const fn twice(n: i32) -> i32 {
    n * 2
}

const TRIANGLE: i32 = twice(<Triangle as Shape>::SIDES);
const SQUARE: i32 = Square::DOUBLE + Wrap::<Square>::INNER;

fn sides<T: Shape>() -> i32 {
    T::SIDES
}

fn doubled<T: Shape>(_: &T) -> i32 {
    <T as Shape>::DOUBLE
}

fn main() {
    say(Triangle::SIDES);
    say(Square::DOUBLE);
    say(<Triangle as Shape>::DOUBLE);
    say(sides::<Square>());
    say(doubled(&Triangle));
    say(Triangle.describe());
    say(Square.describe());
    say(TRIANGLE);
    say(SQUARE);
    say(Wrap(Triangle).inner());
    say(sides::<Wrap<Wrap<Square>>>());
    say(<Wrap<Triangle> as Shape>::DOUBLE);
    let (n, many) = Square::NAME;
    say(n + many as i32);
    say(<Triangle as Scaled<u8>>::SCALE * 10 + <Triangle as Scaled<u16>>::SCALE);
}
