//! `modules/util/bits.rs`: a child of a `mod.rs` module.

pub use super::LIMIT;

pub const HIGH: u64 = 1 << 63;
pub const MASK: u64 = HIGH >> 1 | LIMIT as u64;

pub fn popcount(x: u64) -> u32 {
    x.count_ones()
}

pub const fn parity(x: u64) -> u32 {
    x.count_ones() % 2
}

pub fn describe() -> i64 {
    4
}
