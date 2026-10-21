//! `modules/geometry/extra/more.rs`: a file under an inline module.

use super::super::super::util::LIMIT;
use crate::modules::geometry::{ORIGIN_TAG, shapes::Rect};

pub fn describe() -> i64 {
    ORIGIN_TAG + LIMIT as i64 + Rect::UNIT.area() as i64
}
