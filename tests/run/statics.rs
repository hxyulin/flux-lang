//! Statics: immutable ones of integer, struct, tuple and enum types,
//! with initializers evaluated at compile time from `const fn`s and
//! other statics, read, borrowed and passed by reference; `static mut`
//! read and written in `unsafe` blocks, whole, by field and with
//! compound assignment; constants reading statics, and a static whose
//! initializer indexes another static at computed indices.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

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

static LIMIT: u32 = 10;
static ORIGIN: Point = Point { x: 3, y: -4 };
static DOUBLE: u32 = LIMIT * 2 + square(3);
static mut COUNTER: u64 = 0;
static mut POS: Point = Point { x: 0, y: 0 };
static mut FLAGS: (bool, u8) = (false, 7);
static CHOICE: Option<u8> = Some(9);
const FROM_STATIC: u32 = LIMIT + 1;
static TABLE: [u32; 4] = [5, 6, 7, 8];
static SWAPPED: [u32; 2] = {
    let mut out = [0u32; 2];
    let mut i = 0;
    while i < 2 {
        out[i] = TABLE[2 * i + 1];
        i += 1;
    }
    out
};

const fn square(n: u32) -> u32 {
    n * n
}

fn bump(by: u64) -> u64 {
    unsafe {
        COUNTER += by;
        COUNTER
    }
}

fn read(p: &Point) -> i32 {
    p.x * 10 + p.y
}

fn main() {
    bump(5);
    bump(7);
    unsafe {
        POS.x = 4;
        POS.y += 2;
        FLAGS.0 = !FLAGS.0;
        FLAGS.1 *= 3;
    }
    let p = unsafe { POS };
    let r = &ORIGIN;
    let c = match CHOICE {
        Some(v) => v,
        None => 0,
    };
    unsafe {
        printf(
            c"%u %d %d %u %llu %d %d %d %u %d %d %u\n".as_ptr(),
            LIMIT,
            ORIGIN.x,
            ORIGIN.sum(),
            DOUBLE,
            COUNTER,
            p.x,
            p.y,
            FLAGS.0 as i32,
            FLAGS.1 as u32,
            read(r),
            read(&ORIGIN),
            FROM_STATIC + c as u32,
        );
    }
    unsafe {
        printf(c"%u %u\n".as_ptr(), SWAPPED[0], SWAPPED[1]);
    }
}
