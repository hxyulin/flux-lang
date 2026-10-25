#![allow(dead_code, unused_variables)]

#[derive(Clone, Copy, Default)]
struct Record {
    #[cfg(any())]
    removed: Missing,
    #[cfg_attr(all(), cfg(all()))]
    value: i32,
}

#[derive(Clone, Copy)]
struct Tuple(#[cfg(any())] Missing, i32, #[cfg_attr(all(), cfg(any()))] Missing, i64);

#[derive(Default)]
enum Choice {
    #[cfg(any())]
    #[default]
    Removed(Missing),
    #[cfg_attr(all(), default)]
    Kept,
    Data(#[cfg(any())] Missing, i32),
    Named { #[cfg_attr(all(), cfg(any()))] removed: Missing, value: i32 },
}

enum Number {
    First = 7,
    #[cfg_attr(all(), cfg(any()))]
    Removed = missing(),
    Last,
}

fn take(#[cfg(any())] removed: Missing, #[cfg_attr(all(), cfg(all()))] value: i32) -> i32 {
    value
}

impl Record {
    fn associated(#[cfg(any())] &self, value: i32) -> i32 { value }
    fn method(&self, #[cfg_attr(all(), cfg(any()))] removed: Missing) -> i32 { self.value }
}

trait Signature {
    fn take(#[cfg(any())] (removed, pattern): (Missing, Missing), value: i32) -> i32;
}

type Function = fn(#[cfg_attr(all(), cfg(any()))] Missing, i32) -> i32;

fn main() {
    let r = Record { value: 3 };
    let t = Tuple(5, 9);
    let Tuple(a, b) = t;
    let f: Function = take;
    let closure = |#[cfg_attr(all(), cfg(any()))] removed: Missing, value: i32| value;
    let value = f(a) + closure(r.method()) + Record::associated(1);
}
