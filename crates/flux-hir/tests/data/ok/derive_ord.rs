// `#[derive(PartialOrd, Ord)]`: `partial_cmp` and `cmp` made as rustc
// expands them. A struct compares its fields in order, each through the
// trait's method on references to them, the first that is not `Equal`
// deciding (`Some(Equal)` or `Equal` without fields). An enum compares
// the discriminants of its values, literals of its `repr` type (or a
// discriminant constant plus the variants since), unless both are the
// same variant, whose fields it compares as a struct's.

use core::cmp::Ordering;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Point {
    x: i32,
    y: i32,
    z: u8,
}

#[derive(PartialEq, PartialOrd)]
struct Tuple(u8);

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Unit;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Wrapper<T>(T, T);

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Fieldless {
    A,
    B,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Mixed {
    Unit,
    Tuple(i32, u8),
    Record { a: bool },
}

#[derive(PartialEq, PartialOrd)]
#[repr(i8)]
enum Signed {
    Low = -1,
    High = 5,
    Next,
}

const START: isize = 1;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Computed {
    First = START + 1,
    Second,
}

#[derive(PartialEq, PartialOrd)]
enum Single {
    Only(u8),
}

fn main() {
    let _a = Point { x: 1, y: 2, z: 3 } < Point { x: 1, y: 2, z: 4 };
    let _b: Ordering = Mixed::Unit.cmp(&Mixed::Record { a: true });
}
