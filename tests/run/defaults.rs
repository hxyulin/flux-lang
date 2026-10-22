//! `Default`: the integers', `bool`'s, `()`'s, `Option`'s, tuples' and
//! arrays' defaults, `#[derive(Default)]` on structs (named, tuple, unit,
//! generic) and on enums with a `#[default]` variant, and the methods
//! that use it: `unwrap_or_default`, `get_or_insert_default` and
//! `core::mem::take`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
struct Point {
    x: i32,
    y: i64,
}

#[derive(Default)]
struct Config {
    verbose: bool,
    level: u8,
    origin: Point,
    limits: (u16, i8),
    grid: [[u8; 3]; 2],
    name: Option<u32>,
}

#[derive(Default)]
struct Pair<T>(T, T);

#[derive(Default)]
struct Unit;

#[allow(dead_code)]
#[derive(Default, Clone, Copy, PartialEq)]
enum Mode {
    Fast,
    #[default]
    Normal,
    Slow,
}

struct NoDefault(i32);

#[derive(Default)]
enum Slot<T> {
    #[default]
    Empty,
    Full(T),
}

struct Counter {
    made: u32,
}

impl Default for Counter {
    fn default() -> Counter {
        show(99, 99);
        Counter { made: 7 }
    }
}

fn generic<T: Default>() -> T {
    T::default()
}

fn main() {
    let c = Config::default();
    show(1, c.verbose as i64);
    show(2, c.level as i64);
    show(3, (c.origin.x as i64) + c.origin.y);
    show(4, c.limits.0 as i64 + c.limits.1 as i64);
    show(5, c.grid[1][2] as i64);
    show(6, c.name.is_none() as i64);
    let p: Pair<u64> = Default::default();
    show(7, (p.0 + p.1) as i64);
    let _u: Unit = Default::default();
    show(8, (Mode::default() == Mode::Normal) as i64);
    let m: Mode = generic();
    show(9, (m == Mode::Normal) as i64);
    show(10, matches_empty(Slot::<NoDefault>::default()) as i64);
    let counters: [Counter; 3] = Default::default();
    show(11, (counters[0].made + counters[2].made) as i64);
    let empty: [NoDefault; 0] = Default::default();
    show(12, empty.len() as i64);
    let t: (i32, bool, (u8, Option<i8>)) = Default::default();
    show(13, t.0 as i64 + t.1 as i64 + (t.2).0 as i64);
    let big: [i64; 32] = Default::default();
    show(14, big[31]);

    let some: Option<i32> = Some(5);
    let none: Option<i32> = None;
    show(15, some.unwrap_or_default() as i64);
    show(16, none.unwrap_or_default() as i64);
    let err: Result<u8, i32> = Err(3);
    show(17, err.unwrap_or_default() as i64);
    let mut slot: Option<Point> = None;
    let inserted = slot.get_or_insert_default();
    inserted.x = 4;
    show(18, slot.unwrap().x as i64);
    let mut v = 41;
    let old = core::mem::take(&mut v);
    show(19, old as i64 * 100 + v as i64);
    let mut opt = Some(3);
    let taken = core::mem::take(&mut opt);
    show(20, taken.unwrap() as i64 + opt.is_none() as i64);
    let full = Slot::Full(NoDefault(8));
    if let Slot::Full(n) = full {
        show(21, n.0 as i64);
    }
}

fn matches_empty<T>(s: Slot<T>) -> bool {
    match s {
        Slot::Empty => true,
        Slot::Full(_) => false,
    }
}
