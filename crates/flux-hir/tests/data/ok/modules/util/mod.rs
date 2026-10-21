// A `mod.rs` module, with a child in the same directory.

pub mod helpers;

pub const ZERO: i32 = 0;

pub fn clamp(x: i32, lo: i32, hi: i32) -> i32 {
    x.max(lo).min(hi)
}
