//! `modules/util/mod.rs`: a file module in the `x/mod.rs` layout.

pub mod bits;

/// An inline module whose child is a file: `util/inline/leaf.rs`.
mod inline {
    pub mod leaf;
}

pub use inline::leaf::{Ring as LeafRing, ring};

/// `use super::*` sees the parent's private items too.
mod checks {
    use super::*;

    pub(super) fn bonus_twice() -> i64 {
        private_bonus() * 2 + LIMIT as i64
    }
}

pub const LIMIT: i32 = 50;

pub fn clamp(x: i32, lo: i32, hi: i32) -> i32 {
    x.max(lo).min(hi)
}

pub fn describe() -> i64 {
    3 + private_bonus()
}

pub(self) fn private_bonus() -> i64 {
    0
}

pub(super) fn only_parent() -> i64 {
    bits::describe() * 1000 + tally::tally(2) + checks::bonus_twice()
}

pub struct Counter {
    count: u32,
    pub step: u32,
}

impl Counter {
    pub const fn new(step: u32) -> Counter {
        Counter { count: 0, step }
    }

    pub fn bump(&mut self) -> u32 {
        self.count += self.step;
        self.count
    }

    pub fn count(&self) -> u32 {
        self.count
    }
}

/// The same name as `geometry::shapes::Point`.
pub struct Point(pub i64, pub i64);

impl Point {
    pub fn new(a: i64, b: i64) -> Point {
        Point(a, b)
    }

    pub fn sum(&self) -> i64 {
        self.0 + self.1
    }
}

/// A module and a function with the same name: they live in different
/// namespaces.
pub mod tally {
    pub fn tally(n: i64) -> i64 {
        n * super::LIMIT as i64
    }
}

pub fn tally() -> i64 {
    tally::tally(-1)
}
