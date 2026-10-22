//! `#[repr]` on enums: the discriminant's type, which `as` casts start
//! from, for fieldless enums and enums with fields and written
//! discriminants.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

fn show_u(n: u64) {
    unsafe {
        printf(c"%llu\n".as_ptr(), n);
    }
}

#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
enum Byte {
    Low = 1,
    Mid = 128,
    High,
    Top = 255,
}

#[derive(Clone, Copy)]
#[repr(i8)]
enum Signed {
    Neg = -128,
    MinusOne = -1,
    Zero,
}

#[derive(Clone, Copy)]
#[repr(u64)]
enum Wide {
    Max = 18446744073709551615,
}

#[derive(Clone, Copy)]
#[repr(i128)]
enum Huge {
    Small = -170141183460469231731687303715884105728,
    Big = 170141183460469231731687303715884105727,
}

#[repr(C)]
enum Plain {
    A,
    B,
}

#[repr(u16)]
enum Message {
    Quit = 7,
    Move { x: i64, y: i64 } = 300,
    Write(i64),
}

struct Holder {
    first: Byte,
    second: Signed,
}

fn describe(m: &Message) -> i64 {
    match m {
        Message::Quit => 0,
        Message::Move { x, y } => *x + *y,
        Message::Write(n) => *n,
    }
}

fn main() {
    show(Byte::Low as i64);
    show(Byte::Mid as i64);
    show(Byte::High as i64);
    show(Byte::Top as i64);
    show(Byte::Top as i8 as i64);
    show(Signed::Neg as i64);
    show(Signed::Neg as u8 as i64);
    show(Signed::MinusOne as u32 as i64);
    show(Signed::Zero as i64);
    show_u(Wide::Max as u64);
    show(Wide::Max as i64);
    show((Wide::Max as u128 >> 1) as i64);
    show((Wide::Max as i128 >> 63) as i64);
    show((Huge::Small as i128 >> 120) as i64);
    show((Huge::Big as i128 >> 120) as i64);
    show((Huge::Big as u128 >> 127) as i64);
    show(Plain::B as i64);
    show(Plain::A as u8 as i64);
    let h = Holder {
        first: Byte::High,
        second: Signed::MinusOne,
    };
    show(h.first as i64);
    show(h.second as i64);
    let same = if h.first == Byte::High { 1 } else { 0 };
    show(same);
    let messages = (
        Message::Quit,
        Message::Move { x: 3, y: 4 },
        Message::Write(9),
    );
    show(describe(&messages.0));
    show(describe(&messages.1));
    show(describe(&messages.2));
    let top = Byte::Top;
    let n = match top {
        Byte::Low => 1,
        Byte::Mid | Byte::High => 2,
        Byte::Top => 3,
    };
    show(n);
}
