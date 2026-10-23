// Matching direct, transitive and diamond equalities are one membership proof.
trait P<X, Y> {
    type A;
    type B;
    fn a(&self) -> Self::A;
    fn b(&self) -> Self::B;
}
trait R<X, Y>: P<Y, X, A = X, B = Y> {}
trait S<X, Y>: P<Y, X, A = X, B = Y> {}
trait Q<X, Y>: R<X, Y> + S<X, Y> {}
trait H<X, Y> {
    type Part: P<Y, X, A = X, B = Y>;
    fn part(self) -> Self::Part;
}
struct Value(u16, u32);
impl P<u32, u16> for Value {
    type A = u16;
    type B = u32;
    fn a(&self) -> u16 {
        self.0
    }
    fn b(&self) -> u32 {
        self.1
    }
}
// The same Self has another implementation, with distinct trait arguments.
impl P<u16, u32> for Value {
    type A = u32;
    type B = u16;
    fn a(&self) -> u32 {
        self.1
    }
    fn b(&self) -> u16 {
        self.0
    }
}
impl R<u16, u32> for Value {}
impl S<u16, u32> for Value {}
impl Q<u16, u32> for Value {}
struct Owner(Value);
impl H<u16, u32> for Owner {
    type Part = Value;
    fn part(self) -> Value {
        self.0
    }
}
fn read<T, X, Y>(owner: T) -> (X, Y)
where
    T: H<X, Y>,
    T::Part: Q<X, Y> + P<Y, X, A = X, B = Y>,
{
    let value = owner.part();
    (value.a(), value.b())
}
fn main() {
    let (a, b) = read(Owner(Value(513, 70000)));
    assert_eq!(a, 513);
    assert_eq!(b, 70000);
    let v = Value(17, 90000);
    assert_eq!(<Value as P<u16, u32>>::a(&v), 90000);
    println!("compatible proofs {} {}", a, b);
}
