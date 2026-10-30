trait Source {
    fn values(&self) -> impl Iterator<Item = u32>;
}
struct Forward(u32);
struct Backward(u32);
impl Source for Forward {
    fn values(&self) -> impl Iterator<Item = u32> { 0..self.0 }
}
impl Source for Backward {
    fn values(&self) -> impl Iterator<Item = u32> { (0..self.0).rev() }
}
fn collect<S:Source>(source:&S)->Vec<u32> { source.values().collect() }
trait DefaultSource {
    fn values() -> impl Iterator<Item = u32> { 1..4 }
}
struct Defaulted;
impl DefaultSource for Defaulted {}
trait GenericSource {
    fn values<T:Copy>(value:T)->impl Iterator<Item=T>;
}
impl GenericSource for Forward {
    fn values<T:Copy>(value:T)->impl Iterator<Item=T> { [value,value].into_iter() }
}
fn generic<S:GenericSource,T:Copy>(value:T)->Vec<T> { S::values(value).collect() }
trait BorrowSource {
    fn borrowed<'a>(&'a self)->impl Iterator<Item=&'a u32>;
}
impl BorrowSource for Forward {
    fn borrowed<'a>(&'a self)->impl Iterator<Item=&'a u32> { core::iter::once(&self.0) }
}
fn borrowed<S:BorrowSource>(source:&S)->u32 { source.borrowed().copied().sum() }
fn main() {
    assert_eq!(borrowed(&Forward(17)),17);
    assert_eq!(collect(&Forward(4)),vec![0,1,2,3]);
    assert_eq!(collect(&Backward(4)),vec![3,2,1,0]);
    assert_eq!(Defaulted::values().sum::<u32>(),6);
    assert_eq!(generic::<Forward,u16>(7),vec![7,7]);
    println!("trait opaque returns 0 1 2 3 / 3 2 1 0 / 6 / 7 7");
}
