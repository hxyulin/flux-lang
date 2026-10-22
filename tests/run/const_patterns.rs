//! Constants in patterns: alone, as range ends, of integer types and
//! `bool`, free, associated and of traits, and in `let` conditions.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

const ZERO: i32 = 0;
const SMALL: u8 = 10;
const ON: bool = true;

const fn square(x: u8) -> u8 {
    x * x
}

const BIG: u8 = square(SMALL) + 5;

struct Limits;

impl Limits {
    const LOW: i64 = -100;
    const HIGH: i64 = 100;
}

trait Code {
    const CODE: u16;
}

struct Ok;

impl Code for Ok {
    const CODE: u16 = 200;
}

fn sign(x: i32) -> i64 {
    match x {
        i32::MIN..=-1 => -1,
        ZERO => 0,
        1..=i32::MAX => 1,
    }
}

fn size(x: u8) -> i64 {
    match x {
        0 => 0,
        1..SMALL => 1,
        SMALL..=BIG => 2,
        _ => 3,
    }
}

fn flag(b: bool) -> i64 {
    match b {
        ON => 1,
        false => 0,
    }
}

fn clamp(x: i64) -> i64 {
    match x {
        ..Limits::LOW => Limits::LOW,
        Limits::LOW..=Limits::HIGH => x,
        _ => Limits::HIGH,
    }
}

fn status(code: u16) -> i64 {
    match code {
        <Ok as Code>::CODE => 1,
        400..=499 => 4,
        _ => 5,
    }
}

fn main() {
    show(sign(-7));
    show(sign(0));
    show(sign(42));
    show(size(0));
    show(size(9));
    show(size(10));
    show(size(105));
    show(size(106));
    show(flag(true));
    show(flag(false));
    show(clamp(-500));
    show(clamp(55));
    show(clamp(500));
    show(status(200));
    show(status(404));
    show(status(500));
    if let SMALL = 10 {
        show(10);
    }
    let pair = (ZERO, ON);
    match pair {
        (ZERO, ON) => show(1),
        _ => show(0),
    }
}
