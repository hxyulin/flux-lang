//! `#[derive(Debug)]` and `core`'s `Debug` impls, with `{:?}` and
//! `{:#?}`: structs (named, tuple, unit), enums of each kind of
//! variant, generic and nested ones, `Option`, `Result`, `Ordering`,
//! tuples, arrays, slices, references, ranges and `()`; hand-written
//! impls through `debug_struct`, `debug_tuple`, `debug_list`,
//! `debug_set` and `debug_map`, and options reaching nested values.

use core::cmp::Ordering;
use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug)]
struct Unit;

#[derive(Debug)]
struct Wrapper(u8, &'static str);

#[derive(Debug)]
struct Empty {}

#[derive(Debug)]
struct EmptyTuple();

#[derive(Debug)]
struct Generic<T> {
    value: T,
    list: [T; 2],
}

#[derive(Debug)]
enum Shape {
    Circle { center: Point, radius: u32 },
    Square(Point, u32),
    Nothing,
    Wrapped(Option<Box3>),
}

#[derive(Debug)]
struct Box3 {
    inner: (i8, char, bool),
}

#[derive(Debug)]
enum Never {}

struct Matrix {
    rows: [[i32; 2]; 2],
}

impl fmt::Debug for Matrix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.rows.iter()).finish()
    }
}

struct Pairs;

impl fmt::Debug for Pairs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entry(&"one", &1)
            .key(&"two")
            .value(&Some(2))
            .entries([("three", 3), ("four", 4)])
            .finish()
    }
}

struct Set;

impl fmt::Debug for Set {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entry(&1).entries(2..4).finish()
    }
}

struct Partial;

impl fmt::Debug for Partial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Partial").field("shown", &true).finish_non_exhaustive()
    }
}

struct Custom(i32);

impl fmt::Debug for Custom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Custom").field(&self.0).field(&"x").finish()
    }
}

impl fmt::Display for Custom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "custom#{}", self.0)
        } else {
            f.pad("custom")
        }
    }
}

fn show(p: Point, n: u32) {
    let shapes = [
        Shape::Circle {
            center: p,
            radius: n,
        },
        Shape::Square(p, n + 1),
        Shape::Nothing,
        Shape::Wrapped(Some(Box3 {
            inner: (-1, 'q', true),
        })),
        Shape::Wrapped(None),
    ];
    for s in &shapes {
        println!("{:?}", s);
    }
    println!("{:#?}", shapes);
    println!("{:?} {:?} {:?} {:?} {:?}", Unit, Wrapper(7, "w"), Empty {}, EmptyTuple(), p);
    println!("{:#?} {:#?} {:#?}", Unit, Wrapper(7, "w"), Empty {});
    let g = Generic {
        value: "v",
        list: ["a", "b"],
    };
    println!("{:?}", g);
    println!("{:#?}", g);
    let nested = Generic {
        value: Some(Ok::<i32, ()>(n as i32)),
        list: [None, Some(Err(()))],
    };
    println!("{:?}", nested);
    println!("{:#?}", nested);
}

fn options(a: Option<i32>, b: Result<u8, &str>, c: Result<(), Point>) {
    println!("{:?} {:?} {:?}", a, b, c);
    println!("{:#?}", (a, b, c));
}

fn collections(v: &[i32]) {
    println!("{:?} {:?} {:?}", v, &v[..1], &v[v.len()..]);
    println!("{:#?}", v);
    println!("{:?} {:#?}", [[1, 2], [3, 4]], [(); 0]);
    println!("{:?} {:?} {:?}", (), (1,), (1, "two", '3', 4u8, [5]));
    println!("{:#?}", (1,));
    println!("{:?} {:?}", &&v[0], &mut 5);
    println!("{:?} {:?} {:?} {:?} {:?} {:?}", 1..3, 2.., ..4, ..=5, 6..=7, ..);
    println!("{:?}", Ordering::Less);
    println!("{:?}", [Ordering::Equal, Ordering::Greater]);
}

fn custom(n: i32) {
    println!("{:?}", Matrix { rows: [[n, 2], [3, 4]] });
    println!("{:#?}", Matrix { rows: [[n, 2], [3, 4]] });
    println!("{:?}", Pairs);
    println!("{:#?}", Pairs);
    println!("{:?} {:#?}", Set, Set);
    println!("{:?} {:#?}", Partial, Partial);
    println!("{:?} {:#?}", Custom(n), Custom(n));
    println!("[{}] [{:>10}] [{:<10}] [{:#}]", Custom(n), Custom(n), Custom(n), Custom(n));
    println!("{:5?} {:<4?} {:+?} {:04?}", Some(n), Point { x: n, y: -n }, [n, -n], (n, n));
    println!("{:#x?}", Point { x: 255, y: n });
    let _: Option<Never> = None;
}

fn main() {
    show(Point { x: 1, y: -2 }, 3);
    options(Some(4), Ok(5), Err(Point { x: 0, y: 0 }));
    options(None, Err("bad"), Ok(()));
    collections(&[10, 20, 30]);
    custom(9);
}
