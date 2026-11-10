trait Sink<T> {
    type Result;
    fn finish(self) -> Self::Result;
}
struct Count;
impl Sink<u32> for Count {
    type Result = usize;
    fn finish(self) -> usize {
        7
    }
}
struct Folder<C, F, R>(C, F, Option<R>);
trait Complete<T> {
    type Result;
    fn complete(self) -> Self::Result;
}
impl<T, U, C, F> Complete<T> for Folder<C, F, C::Result>
where
    C: Sink<U>,
    F: Fn(T) -> U,
{
    type Result = C::Result;
    fn complete(self) -> Self::Result {
        self.2.unwrap_or_else(|| self.0.finish())
    }
}
fn call<T, C: Complete<T>>(folder: C) -> C::Result {
    folder.complete()
}
fn main() {
    let folder = Folder(Count, |x: u8| u32::from(x), Some(13usize));
    println!("{}", call::<u8, _>(folder));
}
