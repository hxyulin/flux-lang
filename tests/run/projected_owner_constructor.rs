use std::marker::PhantomData;

trait FixedSize {}
struct Fixed<const N: usize>;
impl<const N: usize> FixedSize for Fixed<N> {}
struct Dynamic;

trait Allocate<D> {
    type Buffer<T>;

    fn fill<T: Copy>(value: T) -> Self::Buffer<T>;
}

struct Heap;

impl<const N: usize> Allocate<Fixed<N>> for Heap {
    type Buffer<T> = [T; N];

    fn fill<T: Copy>(value: T) -> Self::Buffer<T> {
        [value; N]
    }
}

impl Allocate<Dynamic> for Heap {
    type Buffer<T> = Vec<T>;

    fn fill<T: Copy>(value: T) -> Self::Buffer<T> {
        vec![value]
    }
}

struct Vector<T, D, S>(S, PhantomData<(T, D)>);
type Owned<T, D> = Vector<T, D, <Heap as Allocate<D>>::Buffer<T>>;

impl<T: Copy, D: FixedSize> Owned<T, D>
where
    Heap: Allocate<D>,
{
    fn fill(value: T) -> Self {
        Vector(<Heap as Allocate<D>>::fill(value), PhantomData)
    }
}

impl<T: Copy> Owned<T, Dynamic> {
    fn fill(count: usize, value: T) -> Self {
        Vector(vec![value; count], PhantomData)
    }
}

fn build<D: FixedSize>() -> Owned<u32, D>
where
    Heap: Allocate<D>,
{
    Owned::fill(42)
}

fn main() {
    let fixed = build::<Fixed<3>>();
    let dynamic = Owned::<_, Dynamic>::fill(2, 7);
    assert_eq!(fixed.0, [42; 3]);
    assert_eq!(dynamic.0.as_slice(), &[7, 7]);
    println!("{} {}", fixed.0[0], dynamic.0.len());
}
