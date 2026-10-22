//! Discriminants written as expressions other than literals: constants,
//! `const fn` calls, blocks, other variants' discriminants (`Self::A`),
//! forward references, implicit discriminants counted from them, `repr`
//! types, enums with fields and drops, and matches and casts that read
//! the evaluated values.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

const N: isize = 10;
const BASE: u8 = 7;

const fn twice(x: isize) -> isize {
    x * 2
}

const fn doubled_base() -> u8 {
    BASE * 2
}

#[derive(Clone, Copy, PartialEq)]
enum E {
    A = 1 << 2,
    B = N + 1,
    C,
    D = twice(3),
    F = -(2 + 3),
    G,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum R {
    X = 200 + 50,
    Y,
    Z = (u8::MAX - 10),
}

enum K {
    A = {
        let x = 2;
        x * 3
    },
    B = if N > 5 { 10 } else { 20 },
    C = K::A as isize * 100,
    D = Self::B as isize + 1,
}

const FROM_ENUM: isize = K::C as isize + 1;

enum Forward {
    A = Forward::C as isize + 1,
    B = 50,
    C = 10,
}

mod m {
    const HIDDEN: isize = 40;

    pub enum Inner {
        A = HIDDEN + 2,
        B = {
            let x: isize = Self::A as isize;
            x * 2
        },
    }
}

#[derive(Clone, Copy)]
enum Sel {
    A = 3,
    B = Self::A as isize + 1,
}

const fn pick_sel(s: Sel) -> isize {
    match s {
        Sel::A => 1,
        Sel::B => 2,
    }
}

enum UsesMatch {
    X = 7,
    Y = pick_sel(Sel::B) + 100,
}

#[repr(u16)]
enum Message {
    Quit = 1 + 1,
    Move { x: i64, y: i64 } = 300 * 2,
    Write(i64),
}

#[repr(u8)]
enum Generic<T> {
    A = doubled_base(),
    B(T) = BASE + 1,
    C { x: T },
}

struct Loud {
    n: i64,
}

impl Drop for Loud {
    fn drop(&mut self) {
        show(self.n);
    }
}

#[repr(i16)]
enum Holder {
    Empty = -(BASE as i16),
    Full(Loud),
    Two(Loud, Loud) = 1000,
}

#[repr(i32)]
enum Code {
    X = BASE as i32 * 3,
    Y,
}

const SECOND: i32 = Code::Y as i32 + Code::X as i32;

fn pick(e: E) -> i64 {
    match e {
        E::A => 100,
        E::B | E::C => 200,
        E::D => 300,
        E::F => 400,
        E::G => 500,
    }
}

fn describe(m: &Message) -> i64 {
    match m {
        Message::Quit => 0,
        Message::Move { x, y } => *x + *y,
        Message::Write(n) => *n,
    }
}

fn cast(e: E) -> i64 {
    e as i64
}

fn cast_byte(r: R) -> i64 {
    r as i64
}

fn classify(n: i32) -> i64 {
    match n {
        SECOND => 1,
        -7 => 2,
        _ => 3,
    }
}

fn which<T>(g: &Generic<T>) -> i64 {
    match g {
        Generic::A => 10,
        Generic::B(_) => 20,
        Generic::C { .. } => 30,
    }
}

fn make(n: i64) -> Holder {
    if n == 0 {
        Holder::Empty
    } else if n == 1 {
        Holder::Full(Loud { n: 1 })
    } else {
        Holder::Two(Loud { n: 2 }, Loud { n: 3 })
    }
}

const fn tag(h: &Holder) -> i32 {
    match h {
        Holder::Empty => 0,
        Holder::Full(_) => 1,
        Holder::Two(..) => 2,
    }
}

fn holder(n: i64) {
    let h = make(n);
    show(tag(&h) as i64 * 100);
}

fn main() {
    show(E::A as i64);
    show(E::B as i64);
    show(E::C as i64);
    show(E::D as i64);
    show(E::F as i64);
    show(E::G as i64);
    show(cast(E::C));
    show(cast(E::G));
    show(pick(E::C));
    show(pick(E::G));
    show((E::G == E::G) as i64);
    show(R::X as i64);
    show(R::Y as i64);
    show(R::Z as i64);
    show(cast_byte(R::Y));
    show(K::A as i64);
    show(K::B as i64);
    show(K::C as i64);
    show(K::D as i64);
    show(FROM_ENUM as i64);
    show(Forward::A as i64);
    show(Forward::B as i64);
    show(Forward::C as i64);
    show(m::Inner::A as i64);
    show(m::Inner::B as i64);
    show(UsesMatch::X as i64);
    show(UsesMatch::Y as i64);
    show(describe(&Message::Quit));
    show(describe(&Message::Move { x: 3, y: 4 }));
    show(describe(&Message::Write(9)));
    show(which(&Generic::A::<i32>));
    show(which(&Generic::B(5)));
    show(which(&Generic::C { x: 1u8 }));
    show(SECOND as i64);
    show(classify(SECOND));
    show(classify(-7));
    show(classify(0));
    holder(0);
    holder(1);
    holder(2);
}
