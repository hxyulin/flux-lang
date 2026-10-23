trait Parent {}
trait Child: Parent {}
struct S<'a>(&'a i32);
impl Parent for S<'static> {}
impl<'a> Child for S<'a> {}
fn main() {}
