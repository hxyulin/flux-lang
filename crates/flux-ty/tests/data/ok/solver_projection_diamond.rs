struct S;
trait Project {
    type Out;
}
impl Project for S {
    type Out = u8;
}
trait Leaf {}
impl Leaf for u8 {}
trait Left {}
trait Right {}
impl<U: Leaf> Left for S where S: Project<Out = U> {}
impl<U: Leaf> Right for S where S: Project<Out = U> {}
trait Top {
    fn get(&self) -> u8 {
        7
    }
}
impl<T: Left + Right> Top for T {}
fn require<T: Top>(value: &T) -> u8 {
    value.get()
}
fn main() {
    let _: u8 = S.get();
    let _: u8 = S.get();
    let _: u8 = require(&S);
}
