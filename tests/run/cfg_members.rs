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

#[derive(Clone, Copy, Default)]
enum Choice {
    #[cfg(any())]
    #[default]
    Removed(Missing),
    #[cfg_attr(all(), default)]
    Kept,
    Data(#[cfg(any())] Missing, i32),
    Named { #[cfg_attr(all(), cfg(any()))] removed: Missing, value: i32 },
}

#[repr(u8)]
enum Number {
    First = 7,
    #[cfg_attr(all(), cfg(any()))]
    Removed = missing(),
    #[cfg(any())]
    RemovedData(Missing),
    Last,
}

fn take(#[cfg(any())] removed: Missing, value: i32) -> i32 { value + 1 }

impl Record {
    fn associated(#[cfg(any())] &self, value: i32) -> i32 { value + 2 }
    fn method(&self, #[cfg_attr(all(), cfg(any()))] removed: Missing) -> i32 { self.value }
}

type Function = fn(#[cfg_attr(all(), cfg(any()))] Missing, i32) -> i32;

fn choice_value(choice: Choice) -> i32 {
    match choice {
        Choice::Kept => 0,
        Choice::Data(value) => value,
        Choice::Named { value } => value + 1,
    }
}

fn main() {
    let r = Record { value: 3 };
    let cloned = r.clone();
    let t = Tuple(5, 9).clone();
    let Tuple(a, b) = t;
    let f: Function = take;
    let closure = |#[cfg_attr(all(), cfg(any()))] removed: Missing, value: i32| value + 3;
    println!("{} {} {} {} {} {} {} {} {} {} {} {}",
        cloned.method(), a, b, t.0, t.1,
        f(a), closure(10), Record::associated(1),
        Number::Last as u8,
        choice_value(Choice::default()),
        choice_value(Choice::Data(12).clone()),
        choice_value(Choice::Named { value: 13 }));
    println!("{} {} {}", core::mem::size_of::<Record>(), core::mem::size_of::<Tuple>(), Record::default().value);
}
