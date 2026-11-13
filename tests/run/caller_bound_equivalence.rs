trait Source {
    type Out;
}
struct S;
impl Source for S {
    type Out = u32;
}
trait Mark<A> {
    fn value(&self, argument: A) -> A;
}
struct M;
impl Mark<u32> for M {
    fn value(&self, argument: u32) -> u32 {
        argument + 1
    }
}
trait Apply<T: Source>: Mark<T::Out> {
    fn apply(&self, argument: T::Out) -> T::Out {
        self.value(argument)
    }
}
impl<T: Source<Out = u32>, U: Mark<u32>> Apply<T> for U {}
fn forward<T: Source<Out = u32>, U: Mark<T::Out>>(value: &U, argument: u32) -> u32 {
    <U as Apply<T>>::apply(value, argument)
}
fn main() {
    println!("{}", forward::<S, M>(&M, 41));
}
