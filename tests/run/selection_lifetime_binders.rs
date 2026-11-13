trait IntoOwner<'a> { type Out; fn into_owner(self) -> Self::Out; }
struct Owner<'s>(&'s u32);
impl<'s> IntoOwner<'_> for Owner<'s> {
    type Out = &'s u32;
    fn into_owner(self) -> Self::Out { self.0 }
}
trait Seed<'de> { type Out; fn seed(self) -> Self::Out; }
trait Access<'de> {
    type Error;
    fn next<T: Seed<'de>>(&mut self, seed: T) -> Result<Option<T::Out>, Self::Error>;
}
struct Input<'a, R>(&'a mut R);
impl<'de, 'a, 'b: 'a, R: 'b> Access<'de> for Input<'a, R> {
    type Error = ();
    fn next<T: Seed<'de>>(&mut self, seed: T) -> Result<Option<T::Out>, ()> {
        Ok(Some(seed.seed()))
    }
}
struct BorrowSeed<'a>(&'a u32);
impl<'a> Seed<'a> for BorrowSeed<'a> {
    type Out = &'a u32;
    fn seed(self) -> Self::Out { self.0 }
}
fn main() {
    let value = 17;
    let mut storage = 0u32;
    let mut input = Input(&mut storage);
    let selected = input.next(BorrowSeed(&value)).unwrap().unwrap();
    println!("{} {}", *selected, *Owner(&value).into_owner());
}
