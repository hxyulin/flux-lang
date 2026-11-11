use std::borrow::Borrow;
trait Value {
    fn value(&self) -> u8;
}
impl Value for u8 {
    fn value(&self) -> u8 {
        *self
    }
}
type Entry<'a> = (&'a u8, Option<&'a (dyn Value + 'a)>);
trait Valid<'a>: Borrow<[Entry<'a>]> {}
impl<'a, const N: usize> Valid<'a> for [Entry<'a>; N] {}
fn read<'a, T: Valid<'a>>(value: &T) -> u8 {
    let entries = value.borrow();
    *entries[0].0 + entries[0].1.unwrap().value()
}
fn main() {
    let a = 13u8;
    let b = 29u8;
    let entries: [Entry<'_>; 1] = [(&a, Some(&b))];
    assert_eq!(read(&entries), 42);
    println!("{}", read(&entries));
}
