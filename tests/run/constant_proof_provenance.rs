trait P {
    type A;
    const V: Self::A;
}
trait Q: P<A = u16> {}
trait H {
    type Part: P<A = u8>;
}
struct Value;
impl P for Value {
    type A = u16;
    const V: u16 = 513;
}
impl Q for Value {}
struct Owner;
struct Small;
impl P for Small {
    type A = u8;
    const V: u8 = 73;
}
impl H for Owner {
    type Part = Small;
}
struct Direct<T>(T);
impl<T> Direct<T>
where
    T: Q + P<A = u16>,
{
    const V: u16 = T::V;
    fn value() -> u16 {
        T::V
    }
}
struct Associated<T>(T);
impl<T: H> Associated<T>
where
    T::Part: P<A = u8> + P<A = u8>,
{
    const V: u8 = T::Part::V;
    fn value() -> u8 {
        T::Part::V
    }
}
// This declaration equality differs from the initializer's caller equality.
// Rust permits the explicit caller proof to shadow the declaration proof.
trait Header
where
    Self::Part: P<A = u8> + P<A = u8>,
{
    type Part: P<A = u16>;
    const V: u8 = Self::Part::V;
}
fn main() {
    assert_eq!(Direct::<Value>::V, Direct::<Value>::value());
    assert_eq!(Associated::<Owner>::V, Associated::<Owner>::value());
    println!("constant proofs {} {}", Direct::<Value>::V, Associated::<Owner>::V);
}
