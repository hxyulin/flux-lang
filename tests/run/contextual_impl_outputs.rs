trait Once<A> {
    type Out;
    fn once(self, value: A) -> Self::Out;
}
impl<T, A, R> Once<A> for T
where
    T: FnOnce(A) -> R,
{
    type Out = R;
    fn once(self, value: A) -> R { self(value) }
}
trait Mut<A>: Once<A> {
    fn call(&mut self, value: A) -> Self::Out;
}
impl<T, A, R> Mut<A> for T
where
    T: FnMut(A) -> R,
{
    fn call(&mut self, value: A) -> R { self(value) }
}
trait Shared<A>: Mut<A> {
    fn shared(&self, value: A) -> Self::Out;
}
impl<T, A, R> Shared<A> for T
where
    T: Fn(A) -> R,
{
    fn shared(&self, value: A) -> R { self(value) }
}
trait Source { type Out; }
trait Base { type Out; }
impl<R, T: Source<Out = R>> Base for T { type Out = R; }
trait Child: Base { fn get() -> Self::Out; }
impl<T: Source<Out = u32>> Child for T { fn get() -> u32 { 73 } }
struct Input;
impl Source for Input { type Out = u32; }
fn main() {
    println!("{}", Input::get());
    let mut total = 0u32;
    let mut add = |n: u32| { total += n; total };
    println!("{} {}", Mut::call(&mut add, 7), Mut::call(&mut add, 11));
    let factor = 3u32;
    let multiply = |n: u32| n * factor;
    println!("{} {}", Shared::shared(&multiply, 13), Once::once(multiply, 17));
}
