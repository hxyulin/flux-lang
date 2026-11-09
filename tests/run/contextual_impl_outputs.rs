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
trait Source { type Out; fn read(&self) -> Self::Out; }
trait Base { type Out; }
impl<R, T: Source<Out = R>> Base for T { type Out = R; }
trait Child: Base { fn get() -> Self::Out; }
impl<T: Source<Out = u32>> Child for T { fn get() -> u32 { 73 } }
struct Input;
impl Source for Input { type Out = u32; fn read(&self) -> u32 { 73 } }
struct Convert<T>(core::marker::PhantomData<fn() -> T>);
impl<T> Copy for Convert<T> {}
impl<T> Clone for Convert<T> { fn clone(&self) -> Self { *self } }
impl<A: Into<T>, T> Once<A> for Convert<T> {
    type Out = T;
    fn once(self, value: A) -> Self::Out { value.into() }
}
impl<A, T> Mut<A> for Convert<T> where Self: Once<A> {
    fn call(&mut self, value: A) -> Self::Out { self.once(value) }
}
impl<A, T> Shared<A> for Convert<T> where Self: Once<A> {
    fn shared(&self, value: A) -> Self::Out { self.once(value) }
}
struct Lift<T>(T);
impl<R, T: Source<Out = R>> Source for Lift<T> {
    type Out = R;
    fn read(&self) -> R { self.0.read() }
}
trait Forward: Source { fn forward(&self) -> Self::Out; }
impl<T: Source> Forward for Lift<T> {
    fn forward(&self) -> Self::Out { self.read() }
}
trait Finish { type Out; fn finish(self) -> Self::Out; }
struct Ready<T>(T);
impl<T> Finish for Ready<T> { type Out=T; fn finish(self)->T{self.0} }
struct Map<A,F>(A,F);
impl<A:Finish,F:Once<A::Out>> Finish for Map<A,F> {
    type Out=F::Out;
    fn finish(self)->Self::Out{self.1.once(self.0.finish())}
}
struct Flat<A,B>(A,core::marker::PhantomData<B>);
impl<A:Finish> Finish for Flat<A,A::Out> where A::Out:Finish {
    type Out=<A::Out as Finish>::Out;
    fn finish(self)->Self::Out{self.0.finish().finish()}
}
struct Then<A,B,F>(Flat<Map<A,F>,B>);
impl<A,B,F> Finish for Then<A,B,F> where Flat<Map<A,F>,B>:Finish {
    type Out=<Flat<Map<A,F>,B>as Finish>::Out;
    fn finish(self)->Self::Out{self.0.finish()}
}
fn check<O,F:Finish<Out=O>>(f:F)->F{f}
fn chain<A:Finish,B:Finish,F:FnOnce(A::Out)->B>(a:A,f:F)->Then<A,B,F>{
    check::<B::Out,_>(Then(Flat(Map(a,f),core::marker::PhantomData)))
}
fn main() {
    println!("{} {}", Input::get(), Lift(Input).forward());
    let mut convert = Convert::<u32>(core::marker::PhantomData);
    println!("{} {}", Mut::call(&mut convert, 19u16), Shared::shared(&convert, 23u8));
    println!("{}",chain(Ready(7u32),|x|Ready(x+34)).finish());
    let mut total = 0u32;
    let mut add = |n: u32| { total += n; total };
    println!("{} {}", Mut::call(&mut add, 7), Mut::call(&mut add, 11));
    let factor = 3u32;
    let multiply = |n: u32| n * factor;
    println!("{} {}", Shared::shared(&multiply, 13), Once::once(multiply, 17));
}
