macro_rules! discard {
    ($t:ty) => {
        0
    };
}
struct Value<T>(T);
impl<T> Value<T> {
    const EMPTY: [u8; discard!(T)] = [];
}
trait Const<T, const N: usize> {
    fn value() -> usize;
}
impl<T> Const<T, { discard!(T) }> for () {
    fn value() -> usize {
        7
    }
}
fn main() {
    assert_eq!(Value::<String>::EMPTY.len(), 0);
    assert_eq!(<() as Const<String, 0>>::value(), 7);
    println!("discarded generic const macro passed");
}
