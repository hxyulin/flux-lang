trait Base<T: ?Sized> {
    type Borrowed: ?Sized;
}
trait Family<'a, T: ?Sized>: Base<T> {
    type Owned;
    fn from_owned(x: &Self::Owned) -> &T;
    fn as_slice(&self) -> &'a Self::Borrowed;
}
struct S<'a, T>(&'a T);
impl<T> Base<T> for S<'_, T> {
    type Borrowed = T;
}
impl<'a, T> Family<'a, T> for S<'a, T> {
    type Owned = T;
    fn from_owned(x: &Self::Owned) -> &T {
        x
    }
    fn as_slice(&self) -> &'a Self::Borrowed {
        self.0
    }
}
fn main() {
    assert_eq!(*S::from_owned(&5), 5);
}
