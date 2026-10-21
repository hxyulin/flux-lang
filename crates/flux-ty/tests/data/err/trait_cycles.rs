// Cycles of supertraits. rustc stops after these, before checking bodies,
// so they have a fixture of their own.

trait A: B {}

trait B: A {}

trait Itself: Itself {}

// A longer cycle.
trait E: F {}

trait F: G {}

trait G: E {}

// A cycle through a `where Self:` clause.
trait H
where
    Self: I,
{
}

trait I: H {}

// A trait that only uses a cycle is not part of it.
trait User: A {}

fn main() {}
