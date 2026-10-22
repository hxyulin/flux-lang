// Trait object types rustc rejects as written.

trait Tr {}

fn two_lifetimes<'a>(x: &'a (dyn Tr + 'a + 'a)) {}

fn relaxed(x: &(dyn Tr + ?Sized)) {}

// Only the trait's crate may write inherent `impl` blocks for its
// trait objects (E0116).
impl dyn core::fmt::Debug {
    fn helper(&self) {}
}

fn main() {}
