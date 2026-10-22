struct S;
trait Loop {}
impl<T: Loop> Loop for T {}
fn require<T: Loop>(_: T) {}
fn main() {
    require(S);
}
