trait Filtered<'a> {
    type Ret: Iterator<Item = &'a u32>;
    fn filtered(self) -> Self::Ret;
}
impl<'a> Filtered<'a> for &'a [u32] {
    type Ret = std::iter::Filter<std::slice::Iter<'a, u32>, fn(&&u32) -> bool>;
    fn filtered(self) -> Self::Ret {
        fn keep(value: &&u32) -> bool { **value > 3 }
        self.iter().filter(keep as fn(&&u32) -> bool)
    }
}
fn main() { let values = [1u32, 5, 7]; let mut selected = values.as_slice().filtered(); println!("{} {}", selected.next().unwrap(), selected.next().unwrap()); }
