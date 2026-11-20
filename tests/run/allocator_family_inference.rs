use std::marker::PhantomData;
use std::mem::MaybeUninit;

trait Storage<T> {
    fn ptr_mut(&mut self) -> *mut T;
}

trait Allocator<D> {
    type Buffer<T>: Storage<MaybeUninit<T>>;

    fn allocate<T>(dim: D) -> Self::Buffer<T>;
}

struct Heap;
struct Matrix<T, D, S>(S, PhantomData<(T, D)>);
type Uninit<T, D> = Matrix<MaybeUninit<T>, D, <Heap as Allocator<D>>::Buffer<T>>;

impl<T, D> Uninit<T, D>
where
    Heap: Allocator<D>,
{
    fn new(data: <Heap as Allocator<D>>::Buffer<T>) -> Self {
        Self(data, PhantomData)
    }

    fn uninit(dim: D) -> Self {
        Self::new(Heap::allocate(dim))
    }
}

fn make<D>(dim: D) -> Uninit<u32, D>
where
    Heap: Allocator<D>,
{
    let mut matrix = Matrix::uninit(dim);
    unsafe { matrix.0.ptr_mut().write(MaybeUninit::new(42u32)) };
    matrix
}

impl<T> Storage<T> for Box<T> {
    fn ptr_mut(&mut self) -> *mut T {
        &mut **self
    }
}

impl Allocator<()> for Heap {
    type Buffer<T> = Box<MaybeUninit<T>>;

    fn allocate<T>((): ()) -> Self::Buffer<T> {
        Box::new(MaybeUninit::uninit())
    }
}

fn main() {
    let matrix = make(());
    let value = unsafe { matrix.0.assume_init_read() };
    assert_eq!(value, 42);
    println!("{value}");
}
