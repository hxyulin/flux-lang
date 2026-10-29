trait Offset<Start = Self> {
    fn offset(&self, start: &Start) -> usize;
}
trait Stream: Offset<Self::Checkpoint> {
    type Token;
    type Iter: Iterator<Item = (usize, Self::Token)>;
    type Checkpoint;
    fn iter(&self) -> Self::Iter;
    fn checkpoint(&self) -> Self::Checkpoint;
}
struct Checkpoint<I>(I);
impl<'a, T: Clone> Offset<<&'a [T] as Stream>::Checkpoint> for &'a [T] {
    fn offset(&self, start: &<&'a [T] as Stream>::Checkpoint) -> usize {
        start.0.len() - self.len()
    }
}
impl<'a, T: Clone> Stream for &'a [T] {
    type Token = T;
    type Iter = core::iter::Enumerate<core::iter::Cloned<core::slice::Iter<'a, T>>>;
    type Checkpoint = Checkpoint<Self>;
    fn iter(&self) -> Self::Iter {
        (*self).iter().cloned().enumerate()
    }
    fn checkpoint(&self) -> Self::Checkpoint {
        Checkpoint(*self)
    }
}
fn total<S: Stream<Token = u32>>(stream: S) -> usize {
    let start = stream.checkpoint();
    let mut result = stream.offset(&start);
    for (index, value) in stream.iter() {
        result += index + value as usize;
    }
    result
}
struct Owned(Vec<u32>);
impl IntoIterator for Owned {
    type Item = u32;
    type IntoIter = Box<dyn Iterator<Item = u32>>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.0.into_iter())
    }
}
impl<'a> IntoIterator for &'a Owned {
    type Item = &'a u32;
    type IntoIter = Box<dyn Iterator<Item = &'a u32> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.0.iter())
    }
}
fn main() {
    let values = [3, 7, 11, 13];
    let result = total(&values[..]);
    assert_eq!(result, 40);
    let owned = Owned(vec![17, 19, 23]);
    let mut borrowed_sum = 0;
    for item in &owned {
        borrowed_sum += *item;
    }
    let mut moved_sum = 0;
    for item in owned {
        moved_sum += item;
    }
    assert_eq!(borrowed_sum, 59);
    assert_eq!(moved_sum, 59);
    println!("headers {result} {borrowed_sum} {moved_sum}");
}
