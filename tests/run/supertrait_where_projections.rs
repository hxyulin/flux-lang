use std::marker::PhantomData;
trait Length<T> {}
struct Size;
impl<T> Length<T> for Size {}
trait Sequence<T> { type Length: Length<T>; fn item(self) -> T; }
trait Mapped<T, U>: Sequence<T> where Self::Length: Length<U> {
    type Mapped: Sequence<U, Length = Self::Length>;
    fn map(self, value: U) -> Self::Mapped;
}
struct Array<T, N: Length<T>>(T, PhantomData<N>);
impl<T, N: Length<T>> Sequence<T> for Array<T, N> {
    type Length = N;
    fn item(self) -> T { self.0 }
}
impl<T, U, N: Length<T> + Length<U>> Mapped<T, U> for Array<T, N> {
    type Mapped = Array<U, N>;
    fn map(self, value: U) -> Self::Mapped { Array(value, PhantomData) }
}
fn main() {
    let array = Array::<_, Size>(7u32, PhantomData);
    let mapped = array.map(String::from("mapped"));
    assert_eq!(mapped.item(), "mapped");
    println!("mapped");
}
