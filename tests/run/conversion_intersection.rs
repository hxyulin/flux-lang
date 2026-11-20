trait Array {
    type Item;
}

impl<T, const N: usize> Array for [T; N] {
    type Item = T;
}

struct Bag<A: Array>(A);

impl<A: Array> From<A> for Bag<A> {
    fn from(value: A) -> Self {
        Self(value)
    }
}

impl<T: Copy, A: Array<Item = T>> TryFrom<&[T]> for Bag<A> {
    type Error = ();

    fn try_from(_: &[T]) -> Result<Self, Self::Error> {
        Err(())
    }
}

fn main() {
    let data = [1u8, 2];
    assert!(Bag::<[u8; 2]>::try_from(&data[..]).is_err());
    println!("{}", Bag::from(data).0[1]);
}
