trait Error {
    fn code(&self) -> u32;
}
trait Detail: Error {}
struct BorrowedError<'a>(&'a u32);
impl Error for BorrowedError<'_> {
    fn code(&self) -> u32 { *self.0 }
}
impl Detail for BorrowedError<'_> {}

trait Access<'de> {
    type Error: Detail;
    fn error(&self) -> Self::Error;
}
struct Source<'a>(&'a u32);
impl<'de> Access<'de> for Source<'de> {
    type Error = BorrowedError<'de>;
    fn error(&self) -> Self::Error { BorrowedError(self.0) }
}
impl<'de, A: Access<'de> + ?Sized> Access<'de> for &mut A {
    type Error = A::Error;
    fn error(&self) -> Self::Error { (**self).error() }
}

trait Summary {
    type Error: Error;
    fn error(&self) -> Self::Error;
}
struct Wrap<A>(A);
impl<'de, A: Access<'de>> Summary for Wrap<&'de A> {
    type Error = A::Error;
    fn error(&self) -> Self::Error { self.0.error() }
}

fn code<'a, A: Access<'a>>(source: A) -> u32 {
    source.error().code()
}
trait Convert<'a, E> {
    type Output;
    fn convert(self) -> Self::Output;
}
impl<'a, A: Access<'a>> Convert<'a, A::Error> for Wrap<A> {
    type Output = Self;
    fn convert(self) -> Self { self }
}

trait ArraySize {
    type Array<T>: IntoIterator<Item = T>;
}
struct Two;
impl ArraySize for Two {
    type Array<T> = [T; 2];
}
struct Array<T, U: ArraySize>(U::Array<T>);
impl<T, U: ArraySize> IntoIterator for Array<T, U> {
    type Item = T;
    type IntoIter = <U::Array<T> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

trait Base {
    type Out;
    fn out(&self) -> Self::Out;
}
trait Derived: Base<Out = Self::Defined> {
    type Defined: Copy;
}
impl<T: Base> Derived for T where T::Out: Copy {
    type Defined = T::Out;
}
trait Outer: Derived {}
struct Count(u32);
impl Base for Count {
    type Out = u32;
    fn out(&self) -> u32 { self.0 }
}
impl Outer for Count {}
fn inherited<T: Outer>(value: &T) -> T::Defined { value.out() }

fn main() {
    let value = 73;
    let mut source = Source(&value);
    let mut first = &mut source;
    assert_eq!(code(&mut first), 73);
    let wrapped = Wrap(&source);
    let error = Summary::error(&wrapped);
    assert!(core::ptr::eq(error.0, &value));
    let converted = Convert::convert(Wrap(Source(&value)));
    assert_eq!(converted.0.error().code(), 73);
    let array = Array::<_, Two>([&value, &value]);
    let mut iter = array.into_iter();
    assert_eq!(**iter.next().as_ref().unwrap(), 73);
    assert_eq!(*iter.next_back().unwrap(), 73);
    assert!(iter.next().is_none());
    assert_eq!(inherited(&Count(31)), 31);
    println!("associated bounds {}", error.code());
}
