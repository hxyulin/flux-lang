// Traits that cannot be made trait objects (E0038), as rustc reports
// them, and those that can.

trait A {
    fn new() -> Self;
}
trait B {
    fn g<T>(&self, t: T);
}
trait C {
    fn eq(&self, o: &Self) -> bool;
}
trait D: Sized {
    fn f(&self);
}
trait E {
    const N: i32;
}
trait F {
    fn r(&self) -> Self;
}
trait G: PartialEq {}
trait H {
    fn ok(&self);
    fn new() -> Self
    where
        Self: Sized;
    fn g<T>(&self)
    where
        Self: Sized;
}
trait I: B {}
trait J {
    fn by(self);
    fn item(&self) -> Option<Self::X>;
    type X;
}

fn a(_: &dyn A) {}
fn b(_: &dyn B) {}
fn c(_: &dyn C) {}
fn d(_: &dyn D) {}
fn e(_: &dyn E) {}
fn f(_: &dyn F) {}
fn g(_: &dyn G) {}
fn h(_: &dyn H) {}
fn i(_: &dyn I) {}
fn j(_: &dyn J<X = u8>) {}

fn main() {}
