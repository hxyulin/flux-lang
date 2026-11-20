use std::marker::PhantomData;
use std::ops::Add;

trait Allocate<D> {
    type Buffer<T>;

    fn make<T>(value: T) -> Self::Buffer<T>;
}

struct Heap;

struct Dimension;

impl Allocate<Dimension> for Heap {
    type Buffer<T> = [T; 1];

    fn make<T>(value: T) -> Self::Buffer<T> {
        [value]
    }
}

trait Shape<L, R> {
    type Output;
}

struct Same;

impl<D> Shape<D, D> for Same {
    type Output = D;
}

struct Matrix<T, D, S>(T, S, PhantomData<D>);
type Owned<T, D> = Matrix<T, D, <Heap as Allocate<D>>::Buffer<T>>;
type Sum<T, L, R> = Owned<T, <Same as Shape<L, R>>::Output>;

impl<T: Copy + Add<Output = T>, L, R, S, U> Add<Matrix<T, R, U>> for Matrix<T, L, S>
where
    Same: Shape<L, R>,
    Heap: Allocate<<Same as Shape<L, R>>::Output>,
{
    type Output = Sum<T, L, R>;

    fn add(self, rhs: Matrix<T, R, U>) -> Self::Output {
        let value = self.0 + rhs.0;
        Matrix(
            value,
            <Heap as Allocate<<Same as Shape<L, R>>::Output>>::make(value),
            PhantomData,
        )
    }
}

trait ClosedAddition: Sized + Add<Self, Output = Self> {}

impl<T: Copy + Add<Output = T>, D> ClosedAddition for Owned<T, D> where Heap: Allocate<D> {}

fn sum<T: ClosedAddition>(left: T, right: T) -> T {
    left + right
}

fn main() {
    let first: Owned<u32, Dimension> = Matrix(13, [13], PhantomData);
    let second: Owned<u32, Dimension> = Matrix(29, [29], PhantomData);
    let result = sum(first, second);
    assert_eq!(result.1, [42]);
    println!("{}", result.1[0]);
}
