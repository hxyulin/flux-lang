// Bounds outrank blanket impls at the same borrowed receiver candidate.
trait Bound {
    fn pick(&self) -> u8;
}
trait Borrowed {
    fn pick(self) -> u8;
}
struct S;
impl Bound for S {
    fn pick(&self) -> u8 {
        1
    }
}
impl<T> Borrowed for &T {
    fn pick(self) -> u8 {
        2
    }
}
fn pick<T: Bound>(x: T) -> u8 {
    x.pick()
}
trait Child: Bound {}
impl Child for S {}
fn inherited<T: Child>(x: T) -> u8 {
    x.pick()
}
trait Own: Bound {
    fn own(self) -> u8
    where
        Self: Sized,
    {
        self.pick()
    }
}
impl Own for S {}
// Reference bounds still supply by-value borrowed candidates.
fn reference<'a, T>(x: &'a T) -> u8
where
    &'a T: Borrowed,
{
    x.pick()
}
// A bound on the reference does not outrank a bound on T itself.
fn competing<'a, T: Bound + 'a>(x: T) -> u8
where
    &'a T: Borrowed,
{
    x.pick()
}
fn main() {
    println!("{}", pick(S));
    println!("{} {} {} {}", inherited(S), S.own(), reference(&S), competing(S));
}
