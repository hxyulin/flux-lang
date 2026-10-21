//! `Copy`, `Clone`, `PartialEq` and `Eq`: structs that implement `Copy`
//! are copied rather than moved, written by hand or derived; derived
//! `clone` clones each field and derived `==` compares them; generic
//! structs derive them for arguments that implement them; generic code
//! copies `T: Copy` values and clones `T: Clone` ones; integers, `bool`
//! and tuples clone through `core`'s impls; a derived `Clone` with
//! `Copy` copies.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, PartialEq)]
struct Pair<T>(T, T);

#[derive(Clone, PartialEq)]
struct Named {
    id: u8,
    at: Point,
    pair: Pair<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Unit;

// By hand: a `clone` that counts.
struct Counted(i32);

impl Clone for Counted {
    fn clone(&self) -> Counted {
        Counted(self.0 + 1)
    }
}

// A `Copy` field whose `clone` is not a copy: deriving `Clone` with
// `Copy` copies the struct, as rustc's does, while deriving `Clone`
// alone, or on a generic struct, clones each field.
#[derive(Copy)]
struct Odd(i32);

impl Clone for Odd {
    fn clone(&self) -> Odd {
        Odd(self.0 + 100)
    }
}

#[derive(Clone, Copy)]
struct Copied(Odd);

#[derive(Clone)]
struct Cloned(Odd);

#[derive(Clone, Copy)]
struct CopiedGeneric<T>(T);

fn twice<T: Copy>(t: T) -> (T, T) {
    (t, t)
}

fn clones<T: Clone>(t: &T) -> (T, T) {
    (t.clone(), t.clone())
}

fn same<T: Eq>(a: &T, b: &T) -> bool {
    a == b
}

fn sum(p: Point) -> i32 {
    p.x + p.y
}

impl Point {
    fn moved(self, by: i32) -> Point {
        Point {
            x: self.x + by,
            y: self.y + by,
        }
    }
}

fn main() {
    let p = Point { x: 1, y: 2 };
    let q = p;
    let r = p.moved(10);
    let (a, b) = twice(p);
    let s = sum(p) + sum(q) + r.x + a.y + b.x;
    let n = Named {
        id: 7,
        at: p,
        pair: Pair(true, false),
    };
    let m = n.clone();
    let k = Named { id: 8, ..n.clone() };
    let (c1, c2) = clones(&Counted(5));
    let (u1, u2) = twice(Unit);
    let t = (1u8, Pair(3i64, 4)).clone();
    let mut v = Point { x: 0, y: 0 };
    v.clone_from(&r);
    unsafe {
        printf(c"%d %d %d\n".as_ptr(), s, q.x, r.y);
        printf(
            c"%d %d %d %d\n".as_ptr(),
            m.id as i32,
            m.at.y,
            (n == m) as i32,
            (n != k) as i32,
        );
        printf(
            c"%d %d %d %d\n".as_ptr(),
            c1.0,
            c2.0,
            (u1 == u2) as i32,
            same(&p, &Point { x: 1, y: 3 }) as i32,
        );
        printf(
            c"%d %lld %d %d\n".as_ptr(),
            t.0 as i32,
            t.1.0 + t.1.1,
            (Pair(1, 2) == Pair(1, 2)) as i32,
            v.x + 5.clone(),
        );
        printf(
            c"%d %d %d\n".as_ptr(),
            Copied(Odd(1)).clone().0.0,
            Cloned(Odd(1)).clone().0.0,
            CopiedGeneric(Odd(1)).clone().0.0,
        );
    }
}
