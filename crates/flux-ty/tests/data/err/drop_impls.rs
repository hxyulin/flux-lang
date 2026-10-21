// `Drop` impls and calls that rustc rejects: an impl for a struct with
// arguments other than distinct type parameters (E0366), or with bounds
// the struct does not have (E0367), whether in the parameter list or a
// `where` clause; `Copy` for a type with a destructor (E0184); and calls
// of `drop` itself, as a method or by path (E0040). Permuted parameters
// are fine, and so are bounds that hold anyway or that the struct has
// (or implies, through a supertrait). Impls for other types are in
// `drop_targets.rs`.

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {}
}

#[derive(Clone, Copy)]
struct Copied;

impl Drop for Copied {
    fn drop(&mut self) {}
}

struct Specialized<T>(T);

impl Drop for Specialized<i32> {
    fn drop(&mut self) {}
}

struct Twice<A, B>(A, B);

impl<T> Drop for Twice<T, T> {
    fn drop(&mut self) {}
}

struct Swapped<A, B>(A, B);

impl<A, B> Drop for Swapped<B, A> {
    fn drop(&mut self) {}
}

struct Bounded<T>(T);

impl<T: Copy> Drop for Bounded<T> {
    fn drop(&mut self) {}
}

struct Where<T>(T);

impl<T> Drop for Where<T>
where
    T: Clone + PartialEq,
{
    fn drop(&mut self) {}
}

struct Implied<T>(T);

impl<T: Sized> Drop for Implied<T>
where
    Self: Sized,
    i32: Copy,
{
    fn drop(&mut self) {}
}

// The struct's own bounds, permuted, and what they imply.
struct Shared<A: Clone, B: Copy>(A, B);

impl<X: Copy, Y: Clone + Copy> Drop for Shared<Y, X> {
    fn drop(&mut self) {}
}

// One more than the struct's.
struct Fewer<T: Clone>(T);

impl<T: Clone + PartialEq> Drop for Fewer<T> {
    fn drop(&mut self) {}
}

// What a bound says an associated type is, too.
trait Shape {
    type Out;
}

struct Shaped<S: Shape>(S);

impl<S: Shape<Out = i32>> Drop for Shaped<S> {
    fn drop(&mut self) {}
}

struct Fixed<S: Shape<Out = u8>>(S);

impl<S: Shape<Out = u8>> Drop for Fixed<S> {
    fn drop(&mut self) {}
}

fn main() {
    let mut n = Noisy(1);
    n.drop();
    Drop::drop(&mut n);
    Noisy::drop(&mut n);
    <Noisy as Drop>::drop(&mut n);
}
