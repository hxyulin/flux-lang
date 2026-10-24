trait P {
    type A;
    type B;
    const FIXED: u8;
    const VALUE: (Self::A, [Self::A; 2]);
}
struct Value;
impl P for Value {
    type A = u8;
    type B = u32;
    const FIXED: u8 = 73;
    const VALUE: (u8, [u8; 2]) = (11, [17, 23]);
}
struct S<T>(T);
impl<T> S<T>
where
    T: P<A = u8, B = u32> + P<A = u8, B = u64>,
{
    const FIXED: u8 = T::FIXED;
    const VALUE: (u8, [u8; 2]) = T::VALUE;
}
trait H {
    type Part;
}
struct Owner;
impl H for Owner {
    type Part = Value;
}
struct Associated<T>(T);
impl<T: H> Associated<T>
where
    T::Part: P<A = u8, B = u64> + P<A = u8, B = u32>,
{
    const FIXED: u8 = T::Part::FIXED;
    const VALUE: (u8, [u8; 2]) = T::Part::VALUE;
}
// Contradictory generic declarations have no concrete instantiation. Keep
// these as checked declarations, and execute a matching-bound control.
struct Matching<T>(T);
impl<T> Matching<T>
where
    T: P<A = u8, B = u32> + P<A = u8, B = u32>,
{
    const FIXED: u8 = T::FIXED;
    const VALUE: (u8, [u8; 2]) = T::VALUE;
}
fn main() {
    let value = Matching::<Value>::VALUE;
    assert_eq!(value.0, 11);
    assert_eq!(value.1[0], 17);
    assert_eq!(value.1[1], 23);
    println!("constant relevance {} {} {} {}", Matching::<Value>::FIXED, value.0, value.1[0], value.1[1]);
}
