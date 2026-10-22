// Statics, mutable and not, named in bodies, constants and other
// statics, through modules and imports.

struct Point {
    x: i32,
    y: i32,
}

static ORIGIN: Point = Point { x: 0, y: 0 };
static LIMIT: u32 = 10;
static TWICE: u32 = LIMIT * 2;
static mut COUNT: u32 = 0;
const FROM_STATIC: u32 = LIMIT + 1;

mod config {
    pub static NAME_LEN: usize = 4;
    pub(crate) static mut LEVEL: u8 = 1;
}

use config::NAME_LEN;

fn main() {
    let a = ORIGIN.x + ORIGIN.y;
    let b = TWICE + FROM_STATIC;
    let c = &LIMIT;
    let d = NAME_LEN;
    unsafe {
        COUNT += 1;
        config::LEVEL = COUNT as u8;
    }
}
