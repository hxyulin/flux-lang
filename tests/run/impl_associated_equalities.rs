trait Length<T> {}
struct Array<T, N: Length<T>>(T, N);

trait Sequence<T>: Sized {
    type Len: Length<T>;
    type Seq: Sequence<T, Len = Self::Len> + FromIterator<T>;

    fn map<B, U>(
        self,
        lhs: Array<B, Self::Len>,
        value: U,
    ) -> Mapped<Array<B, Self::Len>, B, U>
    where
        Array<B, Self::Len>: Sequence<B, Len = Self::Len> + Mapper<B, U>,
        Self: Mapper<T, U>,
        Self::Len: Length<B> + Length<U>;
}

trait Mapper<T, U>: Sequence<T>
where
    Self::Len: Length<U>,
{
    type Output: Sequence<U, Len = Self::Len>;
}

type Mapped<S, T, U> = <<S as Mapper<T, U>>::Output as Sequence<U>>::Seq;

impl<T, N: Default + Length<T>> Sequence<T> for Array<T, N> {
    type Len = N;
    type Seq = Self;

    fn map<B, U>(
        self,
        _lhs: Array<B, Self::Len>,
        value: U,
    ) -> Mapped<Array<B, Self::Len>, B, U>
    where
        Array<B, Self::Len>: Sequence<B, Len = Self::Len> + Mapper<B, U>,
        Self: Mapper<T, U>,
        Self::Len: Length<B> + Length<U>,
    {
        std::iter::once(value).collect()
    }
}

struct One;
impl<T> Length<T> for One {}

impl<T, N: Default + Length<T>> FromIterator<T> for Array<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Array(values.into_iter().next().unwrap(), N::default())
    }
}

impl Default for One {
    fn default() -> Self {
        One
    }
}

impl<T, U> Mapper<T, U> for Array<T, One> {
    type Output = Array<U, One>;
}

fn main() {
    let result = Array(3_u8, One).map(Array(8_u32, One), "mapped");
    println!("{}", result.0);
}
