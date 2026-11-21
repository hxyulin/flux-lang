use core::marker::PhantomData;

trait Storage<'a> {
    type Container;
}
impl<'a> Storage<'a> for u8 {
    type Container = Vec<&'a u8>;
}
struct Bucket<'a, K: Storage<'a>> {
    _elements: K::Container,
}
trait Marker {
    type Value;
}
struct Erased<T>(PhantomData<T>);
impl<T> Marker for Erased<T> {
    type Value = T;
}
struct Holder<T: 'static>(Option<T>);
struct Packet<const FLAG: bool>(Holder<<Erased<Bucket<'static, u8>> as Marker>::Value>);
fn create() -> Packet<true> {
    Packet(Holder(None))
}
fn forward() -> Packet<true> {
    create()
}
fn main() {
    assert!(forward().0.0.is_none());
}
