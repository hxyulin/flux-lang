trait Value: 'static {
    fn value(&self) -> u32;
}
impl Value for u32 {
    fn value(&self) -> u32 {
        *self
    }
}
struct Rows(Vec<Box<dyn Value>>);
impl<'a> IntoIterator for &'a Rows {
    type Item = &'a dyn Value;
    type IntoIter = core::iter::Map<
        core::slice::Iter<'a, Box<dyn Value>>,
        fn(&'a Box<dyn Value>) -> Self::Item,
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|x| x.as_ref())
    }
}
fn main() {
    let rows = Rows(vec![Box::new(7)]);
    assert_eq!((&rows).into_iter().next().unwrap().value(), 7);
}
