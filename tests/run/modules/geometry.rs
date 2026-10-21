//! Shapes and transforms: a file module in the `x.rs` layout, whose
//! children live in `geometry/`.

pub mod shapes;
pub mod transform;

// Re-exports, one renamed, and a glob re-export.
pub use self::shapes::{Rect, Round};
pub use transform::*;
// A module re-exported under another name.
pub use self::transform as tf;

pub(super) use self::helpers::secret;
use super::util::Counter;

pub const ORIGIN_TAG: i64 = 100;

/// An inline module whose child is a file: `geometry/extra/more.rs`.
pub mod extra {
    pub mod more;
}

/// The same name as `describe` in the crate root, `shapes` and `util`.
pub fn describe() -> i64 {
    helpers::weight(3) + ORIGIN_TAG
}

/// A function called `main` that is not the program's entry point.
pub fn main() -> i64 {
    describe() * 2 + shapes::describe()
}

pub(crate) fn total_area(a: &Rect, b: &Rect) -> i64 {
    (a.area() + b.area()) as i64
}

pub(super) fn census(counter: &mut Counter) -> i64 {
    let first = counter.bump();
    let second = counter.bump();
    (first * 100 + second) as i64
}

mod helpers {
    use super::ORIGIN_TAG;

    pub(super) fn weight(k: i64) -> i64 {
        k * ORIGIN_TAG / 10 + private_offset()
    }

    pub(in crate::modules) fn secret() -> i64 {
        weight(7) + 1
    }

    pub(self) fn private_offset() -> i64 {
        9
    }
}
