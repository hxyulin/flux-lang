use core::marker::PhantomData;

struct Witness<T>(PhantomData<fn(T) -> T>);

impl<T> Witness<T> {
    const fn apply(self, value: T) -> T {
        value
    }
}

trait Provide<T> {
    const VALUE: Witness<T>;
}

impl<T> Provide<T> for () {
    const VALUE: Witness<T> = Witness(PhantomData);
}

const fn qualified<'a, P, T>(value: &'a T) -> &'a T
where
    P: Provide<&'a T>,
{
    <P as Provide<&'a T>>::VALUE.apply(value)
}

fn abbreviated<'a, P, T>(value: &'a T) -> &'a T
where
    P: Provide<&'a T>,
{
    P::VALUE.apply(value)
}

struct Metadata<T>(T);

trait Layout {
    type Info;
    const METADATA: Metadata<Self::Info>;
}

impl<T: Layout + ?Sized> Layout for &T {
    type Info = T::Info;
    const METADATA: Metadata<Self::Info> = T::METADATA;
}

impl Layout for u32 {
    type Info = u32;
    const METADATA: Metadata<u32> = Metadata(43);
}

fn main() {
    let value = 42;
    assert_eq!(*qualified::<(), _>(&value), 42);
    assert_eq!(*abbreviated::<(), _>(&value), 42);
    assert_eq!(<&u32 as Layout>::METADATA.0, 43);
}
