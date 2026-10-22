// Associated types whose traits hold: through bounds, supertraits, the
// bounds of associated types, `where` clauses of `impl` blocks, generic
// `impl` blocks and traits' own `Self`.

trait Tr {
    type Out: Copy;
}
trait Sub: Tr {}
trait Dim {
    type Smaller: Dim;
}
trait Gen<U> {
    type Out;
}
trait Q: Tr {
    fn q(x: <Self as Tr>::Out) -> Self::Out;
}
struct S<T>(T);
struct Wrap<T>(T);
impl<T: Copy> Tr for Wrap<T> {
    type Out = T;
}
impl Tr for u16 {
    type Out = u16;
}
impl Gen<u8> for u16 {
    type Out = u8;
}
impl Tr for S<u8> {
    type Out = <u16 as Tr>::Out;
}
impl<T> S<T>
where
    T: Tr,
{
    fn m(x: <T as Tr>::Out) -> T::Out {
        x
    }
}
fn a<T: Sub>(x: <T as Tr>::Out) -> T::Out {
    x
}
fn b<T: Dim>(x: <T::Smaller as Dim>::Smaller) -> <<T as Dim>::Smaller as Dim>::Smaller {
    x
}
fn c<T: Copy>(x: <Wrap<T> as Tr>::Out) -> T {
    x
}
fn d<T: Gen<U>, U>(x: <T as Gen<U>>::Out) -> <T as Gen<U>>::Out {
    x
}
fn e(x: <u16 as Gen<u8>>::Out) -> u8 {
    let y: <u16 as Tr>::Out = 1;
    let _z: (u8, <S<u8> as Tr>::Out) = (x, y);
    x
}
fn main() {
    let _ = e(1) as i32 + c::<i32>(2) + S::<u16>::m(3) as i32;
}
