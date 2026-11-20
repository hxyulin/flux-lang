struct Location<'a>(&'a [u32]);
struct Features<'a>(&'a [u32]);
impl<'a> From<&'a [u32]> for Location<'a> {
    fn from(data: &'a [u32]) -> Self {
        Self(data)
    }
}
trait Provider<'a>: Sized {
    fn features(&self, location: impl Into<Location<'a>>) -> Features<'a>;
}
struct Font;
impl<'a> Provider<'a> for Font {
    fn features(&self, location: impl Into<Location<'a>>) -> Features<'a> {
        Features(location.into().0)
    }
}
fn through_trait<'a, T: Provider<'a>>(font: &T, row: &'a [u32]) -> Features<'a> {
    font.features(row)
}
fn main() {
    let data = [37u32, 42];
    println!("{}", through_trait(&Font, &data).0[1]);
}
