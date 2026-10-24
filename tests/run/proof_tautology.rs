trait R {
    type X;
}
trait P<X> {
    type A;
    fn get(&self) -> Self::A;
}
fn forward<T, U>(x: T) -> u8
where
    U: R<X = u8>,
    T: P<u8, A = u8> + P<U::X, A = <T as P<U::X>>::A>,
{
    P::<u8>::get(&x)
}
fn reverse<T, U>(x: T) -> u8
where
    U: R<X = u8>,
    T: P<U::X, A = <T as P<U::X>>::A> + P<u8, A = u8>,
{
    P::<u8>::get(&x)
}
struct Arg;
impl R for Arg {
    type X = u8;
}
struct Value(u8);
impl P<u8> for Value {
    type A = u8;
    fn get(&self) -> u8 {
        self.0
    }
}
fn main() {
    let a = forward::<Value, Arg>(Value(73));
    let b = reverse::<Value, Arg>(Value(73));
    assert_eq!(a, 73);
    assert_eq!(a, b);
    println!("{}", a);
}
