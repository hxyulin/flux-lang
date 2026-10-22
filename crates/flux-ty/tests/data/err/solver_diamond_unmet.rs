struct S;
trait Leaf {}
trait Left {}
trait Right {}
trait Top {}
impl<T: Leaf> Left for T {}
impl<T: Leaf> Right for T {}
impl<T: Left + Right> Top for T {}
fn require<T: Top>(_: T) {}
fn main() {
    require(S);
    require(S);
}
