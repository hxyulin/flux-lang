use std::ops::{Deref, DerefMut};
unsafe trait Bytes: Deref<Target = [u8]> + Sized {}
unsafe impl<'a> Bytes for &'a [u8] {}
unsafe impl<'a> Bytes for &'a mut [u8] {}
unsafe trait IntoBytes<'a>: Bytes {
    fn bytes(self) -> &'a [u8];
}
unsafe impl<'a> IntoBytes<'a> for &'a [u8] {
    fn bytes(self) -> &'a [u8] {
        self
    }
}
unsafe impl<'a> IntoBytes<'a> for &'a mut [u8] {
    fn bytes(self) -> &'a [u8] {
        self
    }
}
unsafe trait IntoMut<'a>: IntoBytes<'a> + DerefMut {
    fn bytes_mut(self) -> &'a mut [u8];
}
unsafe impl<'a> IntoMut<'a> for &'a mut [u8] {
    fn bytes_mut(self) -> &'a mut [u8] {
        self
    }
}
fn borrow<'a, T: IntoBytes<'a>>(x: T) -> &'a [u8] {
    x.bytes()
}
fn borrow_mut<'a, T: IntoMut<'a>>(x: T) -> &'a mut [u8] {
    x.bytes_mut()
}
fn main() {
    let mut values = [1, 2];
    assert_eq!(borrow(&values[..]), [1, 2]);
    borrow_mut(&mut values[..])[1] = 9;
    assert_eq!(values, [1, 9]);
    println!("inherited byte slice traits passed");
}
