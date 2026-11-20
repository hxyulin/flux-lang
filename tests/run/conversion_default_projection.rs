use std::marker::PhantomData;

trait Raw {
    type Numeric;
}

trait Flag: Raw {
    fn bits(self) -> Self::Numeric;
}

struct Bits<T, N = <T as Raw>::Numeric>(N, PhantomData<T>);

impl<T: Flag> Bits<T> {
    fn from_bits(bits: T::Numeric) -> Self {
        Self(bits, PhantomData)
    }
}

impl<T: Flag> From<T> for Bits<T> {
    fn from(value: T) -> Self {
        Self::from_bits(value.bits())
    }
}

impl<T: Flag<Numeric = u8>> TryFrom<u8> for Bits<T> {
    type Error = ();

    fn try_from(bits: u8) -> Result<Self, Self::Error> {
        Ok(Self::from_bits(bits))
    }
}

struct Mark;

impl Raw for Mark {
    type Numeric = u8;
}

impl Flag for Mark {
    fn bits(self) -> u8 {
        3
    }
}

fn main() {
    println!("{} {}", Bits::<Mark>::from(Mark).0, Bits::<Mark>::try_from(7u8).unwrap().0);
}
