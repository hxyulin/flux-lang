//! Derived traits on a tuple struct whose last field is a ?Sized parameter.
#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Wrapper<T: ?Sized>(pub T);
impl<T: ?Sized> Wrapper<T> {
    pub fn wrap(value: &T) -> &Self { unsafe { &*(value as *const T as *const Self) } }
}
fn main() { let w: &Wrapper<str> = Wrapper::wrap("hi"); println!("{:?} {}", w, w == w); }
