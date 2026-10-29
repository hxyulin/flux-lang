trait R {
    type X;
}
trait P<X> {
    type A;
    fn get(&self) -> Self::A;
}
trait Q<X>: P<X, A = u8> {}
fn read<T, U, V>(x: T) -> u8
where
    U: R<X = u8>,
    V: R<X = T>,
    T: P<u8, A = u8> + Q<U::X>,
    V::X: P<u8, A = u8>,
{
    P::<u8>::get(&x)
}
fn distinct<T, U>(x: T) -> (u8, u16)
where
    U: R<X = u16>,
    T: P<u8, A = u8> + P<u16, A = u16> + P<U::X, A = u16>,
{
    (P::<u8>::get(&x), P::<u16>::get(&x))
}
fn duplicates<T, U>(value: T) -> u8
where
    U: R<X = u8>,
    T: P<u8, A = u8> + P<U::X, A = u8>,
{
    P::<u8>::get(&value)
}
fn tautological_reverse<T, U>(value: T) -> u8
where
    U: R<X = u8>,
    T: P<U::X, A = <T as P<U::X>>::A> + P<u8, A = u8>,
{
    P::<u8>::get(&value)
}
struct Value(u8);
impl P<u8> for Value {
    type A = u8;
    fn get(&self) -> u8 {
        self.0
    }
}
impl P<u16> for Value {
    type A = u16;
    fn get(&self) -> u16 {
        self.0 as u16 + 256
    }
}
impl Q<u8> for Value {}
struct Arg;
impl R for Arg {
    type X = u8;
}
struct OtherArg;
impl R for OtherArg {
    type X = u16;
}
struct Owner;
impl R for Owner {
    type X = Value;
}
fn main() {
    let a = read::<Value, Arg, Owner>(Value(73));
    let (b, c) = distinct::<Value, OtherArg>(Value(17));
    assert_eq!(a, 73);
    assert_eq!(b, 17);
    assert_eq!(c, 273);
    let d = duplicates::<Value, Arg>(Value(83));
    let e = tautological_reverse::<Value, Arg>(Value(84));
    assert_eq!((d, e), (83, 84));
    println!("normalized proofs {} {} {} {} {}", a, b, c, d, e);
}
