// Trait object types rustc rejects as written.

trait Tr {}

fn two_lifetimes<'a>(x: &'a (dyn Tr + 'a + 'a)) {}

fn relaxed(x: &(dyn Tr + ?Sized)) {}

fn main() {}
