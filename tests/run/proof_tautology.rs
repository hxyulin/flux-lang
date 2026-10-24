trait R {
    type X;
}
trait P<X> {
    type A;
    fn get(&self) -> Self::A;
}
trait Q<X>: P<X, A = u8> {}

fn signature_forward<T, U, V>(x: T) -> u8
where
    U: R<X = u8>,
    V: R<X = T>,
    T: P<u8, A = u8>,
    V::X: Q<u8> + P<U::X, A = <V::X as P<U::X>>::A>,
{
    P::<u8>::get(&x)
}
fn signature_reverse<T, U, V>(x: T) -> u8
where
    U: R<X = u8>,
    V: R<X = T>,
    T: P<u8, A = u8>,
    V::X: P<U::X, A = <V::X as P<U::X>>::A> + Q<u8>,
{
    P::<u8>::get(&x)
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
impl Q<u8> for Value {}
struct Receiver;
impl R for Receiver {
    type X = Value;
}
fn main() {
    let a = forward::<Value, Arg>(Value(73));
    let b = reverse::<Value, Arg>(Value(73));
    assert_eq!(a, 73);
    assert_eq!(a, b);
    assert_eq!(signature_forward::<Value, Arg, Receiver>(Value(37)), 37);
    assert_eq!(signature_reverse::<Value, Arg, Receiver>(Value(41)), 41);
    println!("{}", a);
}
