struct Rows(Vec<Box<u32>>);
impl<'a> IntoIterator for &'a Rows {
    type Item = &'a u32;
    type IntoIter =
        core::iter::Map<core::slice::Iter<'a, Box<u32>>, fn(&'a Box<u32>) -> Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|x| x.as_ref())
    }
}
fn main() {
    let rows = Rows(vec![Box::new(7)]);
    assert_eq!((&rows).into_iter().next(), Some(&7));
}
