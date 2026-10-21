pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }
}

pub fn origin() -> Point {
    Point::new(0, super::super::util::ZERO)
}
