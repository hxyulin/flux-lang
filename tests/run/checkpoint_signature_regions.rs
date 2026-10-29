use core::marker::PhantomData;
trait Stream {
    type Checkpoint;
}
struct Checkpoint<I, S>(I, PhantomData<S>);
impl<T> Stream for &[T] {
    type Checkpoint = Checkpoint<Self, Self>;
}
trait Offset<Start = Self> {
    fn offset(&self, start: &Start) -> usize;
}
impl<T> Offset for &[T] {
    fn offset(&self, _: &Self) -> usize {
        31
    }
}
impl<'a, T> Offset<<&'a [T] as Stream>::Checkpoint> for &'a [T] {
    fn offset(&self, _: &<&'a [T] as Stream>::Checkpoint) -> usize {
        47
    }
}
fn projected<'a, T>(values: &'a [T], start: &Checkpoint<&'a [T], &'a [T]>) -> usize {
    Offset::offset(&values, start)
}
struct Pair<'a, 'b, T>(&'a [T], &'b [T]);
impl<'left, 'right, T> Stream for Pair<'left, 'right, T> {
    type Checkpoint = Checkpoint<&'right [T], &'left [T]>;
}
impl<'a, 'b, T> Offset<<Pair<'a, 'b, T> as Stream>::Checkpoint> for Pair<'a, 'b, T> {
    fn offset(&self, start: &Checkpoint<&'b [T], &'a [T]>) -> usize {
        self.0.len() + self.1.len() + start.0.len()
    }
}
fn main() {
    let values = [3u32, 7, 11];
    let slice = &values[..];
    let start = Checkpoint(slice, PhantomData::<&[u32]>);
    assert_eq!(Offset::offset(&slice, &slice), 31);
    assert_eq!(projected(slice, &start), 47);
    let other = [13u32, 17];
    let pair = Pair(slice, &other[..]);
    let permuted = Checkpoint(&other[..], PhantomData::<&[u32]>);
    assert_eq!(Offset::offset(&pair, &permuted), 7);
    println!(
        "checkpoint {} {} {}",
        Offset::offset(&slice, &slice),
        projected(slice, &start),
        Offset::offset(&pair, &permuted)
    );
}
