// Associated constants that code without generic parameters names are
// evaluated, used or not, as rustc does, with the arguments it gives, so
// their errors are reported (E0080, at the constant that failed), and
// the overflow lints know their values.

trait Tr {
    const N: u8;
    const HALF: u8 = Self::N / 2;
}

struct Zero;

impl Tr for Zero {
    const N: u8 = 0;
}

struct Max;

impl Tr for Max {
    const N: u8 = u8::MAX;
}

struct Broken;

impl Tr for Broken {
    const N: u8 = 1 / 0;
}

struct W<T>(T);

impl<T: Tr> W<T> {
    const INV: u8 = 10 / T::N;
}

fn unused() -> u8 {
    <Broken as Tr>::HALF
}

fn main() {
    let a = W::<Zero>::INV;
    let b = Max::N + 1;
    let c = 10 / Zero::N;
}
