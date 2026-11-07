trait Borrowed<'a, T: 'a>: Iterator<Item = &'a T> {
    fn clone_box(&self) -> Box<dyn Borrowed<'a, T> + 'a>;
}
impl<'a, T> Borrowed<'a, T> for std::slice::Iter<'a, T> {
    fn clone_box(&self) -> Box<dyn Borrowed<'a, T> + 'a> { Box::new(self.clone()) }
}
fn main() {
    let array = [3u32, 5, 7];
    let mut values: Box<dyn Borrowed<'_, u32>> = Box::new(array.iter());
    assert_eq!(values.next(), Some(&3));
    let mut other = values.clone_box();
    assert_eq!(other.next(), Some(&5));
    println!("{}", values.next().unwrap());
}
