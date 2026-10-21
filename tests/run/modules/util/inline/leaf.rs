//! `modules/util/inline/leaf.rs`: a file under an inline module in a
//! `mod.rs` file, re-exported by `util` past the private `inline`.

/// Glob imports that import each other.
pub mod ring_a {
    pub use super::ring_b::*;

    pub fn a() -> i64 {
        1
    }
}

pub mod ring_b {
    pub use super::ring_a::*;

    pub fn b() -> i64 {
        20
    }
}

pub struct Ring {
    pub size: i64,
}

pub fn ring() -> Ring {
    Ring {
        size: ring_a::b() + ring_b::a() * 300 + ring_a::a() * 4000 + ring_b::b(),
    }
}
