// Matching explicit and declaration equalities remain one usable method proof.
trait P {
    type A;
    fn get(&self) -> Self::A;
}
trait Q: P<A = u16> {}
trait H {
    type Part: P<A = u16>;
    fn part(self) -> Self::Part;
}
struct Part(u16);
impl P for Part {
    type A = u16;
    fn get(&self) -> u16 {
        self.0
    }
}
impl Q for Part {}
struct Holder(Part);
impl H for Holder {
    type Part = Part;
    fn part(self) -> Part {
        self.0
    }
}
fn f<T: H>(h: T) -> u16
where
    T::Part: Q,
{
    h.part().get()
}
fn main() {
    let value = f(Holder(Part(513)));
    assert_eq!(value, 513);
    println!("projection equality {}", value);
}
