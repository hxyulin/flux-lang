trait Filtered<'a> {
    type Ret: Iterator<Item = &'a u32>;
}
impl<'a> Filtered<'a> for &'a [u32] {
    type Ret = std::iter::Filter<std::slice::Iter<'a, u32>, fn(&'static &'a u32) -> bool>;
}
fn main() {}
