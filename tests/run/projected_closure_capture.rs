trait View {
    fn get(&self, i: usize) -> &u32;
    fn count(&self) -> usize;
}
impl View for [u32] {
    fn get(&self, i: usize) -> &u32 {
        &self[i]
    }
    fn count(&self) -> usize {
        self.len()
    }
}
trait Family<'a> {
    type Slice: View + ?Sized;
}
struct Holder<'a, T: Family<'a>> {
    slice: &'a T::Slice,
}
impl<'a, T: Family<'a>> Holder<'a, T> {
    fn iter(self) -> impl Iterator<Item = &'a u32> {
        (0..self.slice.count()).map(move |i| self.slice.get(i))
    }
}
struct Key;
impl Family<'_> for Key {
    type Slice = [u32];
}
fn main() {
    let data = [2, 3, 5];
    assert_eq!(Holder::<Key> { slice: &data }.iter().sum::<u32>(), 10);
}
