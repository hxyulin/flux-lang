//! `modules/geometry/shapes.rs`: structs with private fields.

use crate::modules::util::clamp;

pub struct Rect {
    w: i32,
    h: i32,
}

impl Rect {
    pub const MAX_SIDE: i32 = 1000;
    pub const UNIT: Rect = Rect { w: 1, h: 1 };

    pub fn new(w: i32, h: i32) -> Rect {
        Rect {
            w: clamp(w, 0, Self::MAX_SIDE),
            h: clamp(h, 0, Self::MAX_SIDE),
        }
    }

    pub fn area(&self) -> i32 {
        self.w * self.h
    }

    pub fn width(&self) -> i32 {
        self.w
    }

    pub(crate) fn height(&self) -> i32 {
        self.h
    }

    pub(super) fn set_width(&mut self, w: i32) {
        self.w = clamp(w, 0, Self::MAX_SIDE);
    }
}

/// A public field beside a private one.
pub struct Round {
    pub r: i32,
    id: u32,
}

impl Round {
    pub const fn new(r: i32) -> Round {
        Round { r, id: next_id(r) }
    }

    pub fn id(&self) -> u32 {
        self.id
    }
}

const fn next_id(r: i32) -> u32 {
    (r as u32).wrapping_mul(2654435761) >> 16
}

/// The same name as `util::Point`, which is a tuple struct.
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn norm1(&self) -> i32 {
        self.x.abs() + self.y.abs()
    }
}

pub fn describe() -> i64 {
    2
}
