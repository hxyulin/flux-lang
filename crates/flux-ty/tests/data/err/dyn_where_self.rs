// `where Self: PartialEq` uses `Self` as a type parameter, as a
// supertrait bound does.
trait Tr
where
    Self: PartialEq,
{
}
impl Tr for i32 {}

fn main() {
    let a: &dyn Tr = &1i32;
}
