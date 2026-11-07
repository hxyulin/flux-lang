trait Borrowed<'a>: Iterator<Item = &'a u32> {}
impl<'a> Borrowed<'a> for std::slice::Iter<'a, u32> {}
fn leak<'a>() -> Box<dyn Borrowed<'a> + 'a> {
    let local = [1u32];
    Box::new(local.iter())
}
fn main() {}
