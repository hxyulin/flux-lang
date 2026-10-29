// A parameter that appears only in a trait object's arguments or
// associated type bindings is constrained.
trait Other<T> {}
trait Assoc {
    type Out;
}
trait Tr {}
impl<T> Tr for Box<dyn Other<T>> {}
impl<U> Tr for Box<dyn Assoc<Out = U>> {}

fn main() {}
