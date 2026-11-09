trait Make {
    fn make(&self) -> (Option<impl Iterator<Item = u32>>, impl Copy);
}
struct Source(u32);
impl Make for Source {
    fn make(&self) -> (Option<impl Iterator<Item = u32>>, impl Copy) {
        (Some(core::iter::once(self.0)), self.0)
    }
}
fn total<T: Make>(value: &T) -> u32 {
    value.make().0.unwrap().sum()
}
fn main() {
    println!("{}", total(&Source(42)));
}
