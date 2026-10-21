// `#[derive]` on structs: each of `Clone`, `Copy`, `PartialEq` and `Eq`
// becomes an `impl` block of the trait from `core`, generic over the
// struct's parameters with a bound of the trait on each, with a method
// whose body is made from the fields: `clone` clones each field, `eq`
// compares them with `==` joined by `&&` (`true` without fields), and
// `Eq`'s check calls `assert_receiver_is_total_eq` on each.

// Record, tuple and unit structs.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Clone, PartialEq)]
struct Tuple(u8, bool);

#[derive(Clone, Copy, PartialEq, Eq)]
struct Unit;

#[derive(Clone, PartialEq)]
struct Empty {}

#[derive(Clone, PartialEq)]
struct EmptyTuple();

// Generic structs: a bound per parameter.
#[derive(Clone, Copy)]
struct Pair<T>(T, T);

#[derive(PartialEq, Eq)]
struct Two<A, B> {
    a: A,
    b: B,
}

// One trait per attribute, a trailing comma, other attributes and doc
// comments around the derive, and a derive on a `pub` struct in a module.
#[derive(Clone)]
#[derive(PartialEq,)]
/// A struct with its derives split up.
#[allow(dead_code)]
struct Split {
    inner: Point,
}

mod shapes {
    #[derive(Clone, Copy)]
    pub struct Square {
        pub side: u32,
    }
}

fn main() {
    let p = Point { x: 1, y: 2 };
    let _q = p.clone() == p;
    let _s = shapes::Square { side: 3 }.clone();
}
