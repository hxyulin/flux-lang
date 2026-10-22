// Statics: read, borrowed and passed by reference; `static mut` read
// and written in `unsafe` blocks, whole and by field; statics in
// constants and other statics' values.

#[derive(Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn sum(&self) -> i32 {
        self.x + self.y
    }
}

const fn square(n: u32) -> u32 {
    n * n
}

static ORIGIN: Point = Point { x: 1, y: 2 };
static LIMIT: u32 = square(3) + 1;
static TWICE: u32 = LIMIT * 2;
static mut COUNT: u64 = 0;
static mut POS: Point = Point { x: 0, y: 0 };
static NOTHING: () = ();
static CHOICE: Option<u8> = Some(3);
const FROM_STATIC: u32 = TWICE + 1;

fn read(p: &Point) -> i32 {
    p.x
}

fn bump() -> u64 {
    unsafe {
        COUNT += 1;
        COUNT
    }
}

unsafe fn reset() {
    unsafe {
        COUNT = 0;
        POS = ORIGIN;
    }
}

fn main() {
    let a = ORIGIN.sum() + read(&ORIGIN);
    let r = &LIMIT;
    let b = *r + TWICE + FROM_STATIC;
    let c = bump();
    let p = unsafe {
        POS.x += 1;
        POS.y = POS.x * 2;
        POS
    };
    let n = NOTHING;
    let o = CHOICE.unwrap_or(0);
    unsafe { reset() };
}
