use std::fmt::Debug;
use std::mem;

fn require_static<T: 'static>() {}
#[derive(Debug)]
struct U<T: 'static>(T);
trait Mark {
    fn bytes(&self) -> usize;
}
impl<T> Mark for U<T> {
    fn bytes(&self) -> usize {
        require_static::<T>();
        mem::size_of::<T>()
    }
}
trait HeaderArgument<A> {
    fn bytes() -> usize;
}
impl<T> HeaderArgument<U<T>> for () {
    fn bytes() -> usize {
        require_static::<T>();
        mem::size_of::<T>()
    }
}
fn consume<T>(value: U<T>) -> usize {
    require_static::<T>();
    value.bytes()
}
fn make<T>(value: T) -> U<T> {
    require_static::<T>();
    U(value)
}
fn borrowed<T>(value: &U<T>) -> usize {
    require_static::<T>();
    value.bytes()
}
trait Endian: Copy + Debug + 'static {}
impl Endian for u32 {}
trait Header {
    type E: Endian;
}
struct H;
impl Header for H {
    type E = u32;
}
#[derive(Debug)]
struct F<T: Header>(U<T::E>);
impl<T: Header> Mark for F<T> {
    fn bytes(&self) -> usize {
        self.0.bytes()
    }
}
fn main() {
    let value = make(37u32);
    assert_eq!(borrowed(&value), 4);
    assert_eq!(consume(value), 4);
    assert_eq!(<() as HeaderArgument<U<u64>>>::bytes(), 8);
    let projected = F::<H>(U(73u32));
    assert_eq!(projected.bytes(), 4);
    println!(
        "static-wf {} {} {}",
        projected.0.0,
        projected.bytes(),
        <() as HeaderArgument<U<u64>>>::bytes()
    );
}
