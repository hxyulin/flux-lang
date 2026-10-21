//! `modules/geometry/transform/mod.rs`, with a child in `matrix.rs`.

mod matrix;

pub use matrix::{IDENTITY_SCALE, Matrix};

use super::super::util::bits;
use super::shapes::Rect;

pub fn apply(m: &Matrix, r: &Rect) -> Rect {
    Rect::new(r.width() * m.sx(), r.height() * m.sy())
}

pub fn checksum(r: &Rect) -> u32 {
    bits::popcount(r.area() as u64) + bits::parity(r.width() as u64)
}

// An impl block for `Rect` in another module than `Rect`'s.
impl Rect {
    pub fn transposed(&self) -> Rect {
        Rect::new(self.height(), self.width())
    }

    pub fn widen(&mut self, by: i32) {
        let w = self.width() + by;
        self.set_width(w);
    }
}
