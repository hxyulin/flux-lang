trait Visit<'a> {
    type Value;
    fn visit(self, bytes: &'a [u8]) -> Self::Value;
}
struct Visitor;
impl<'a> Visit<'a> for Visitor {
    type Value = usize;
    fn visit(self, bytes: &'a [u8]) -> usize {
        bytes.iter().map(|x| *x as usize).sum()
    }
}
trait Read<'a> {
    fn read<V: Visit<'a>>(self, visitor: V) -> V::Value;
}
struct Data<'a>(&'a [u8]);
impl<'a> Read<'a> for &mut Data<'a> {
    fn read<V: Visit<'a>>(self, visitor: V) -> V::Value {
        let bytes: &'a [u8] = self.0;
        visitor.visit(bytes)
    }
}
trait Pair<'a> {
    fn pair<'b>(self, other: &'b [u8]) -> (&'a [u8], &'b [u8]);
}
impl<'a> Pair<'a> for &mut Data<'a> {
    fn pair<'b>(self, other: &'b [u8]) -> (&'a [u8], &'b [u8]) {
        let first: &'a [u8] = self.0;
        let second: &'b [u8] = other;
        (first, second)
    }
}
fn main() {
    let input = [3, 7, 11];
    let mut data = Data(&input);
    assert_eq!((&mut data).read(Visitor), 21);
    let other = [19, 23];
    let (first, second) = (&mut data).pair(&other);
    assert_eq!(first.as_ptr(), input.as_ptr());
    assert_eq!(second.as_ptr(), other.as_ptr());
    println!("named owner {} {}", first[2], second[1]);
}
