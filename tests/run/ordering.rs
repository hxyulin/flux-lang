//! `Ordering`, `PartialOrd` and `Ord`: comparisons of integers, `bool`,
//! references, `Option`, `Result` and `Ordering` itself through `core`'s
//! impls, `#[derive(PartialOrd, Ord)]` on structs and enums (fields in
//! order, variants by discriminant), `cmp`, `max`, `min` and `clamp`,
//! `core::cmp::{max, min}`, generic code bounded by the traits, and
//! values with destructors, of which `max` and `min` drop the other;
//! last, a `clamp` whose bounds are reversed, which panics.

use core::cmp::{self, Ordering};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: *const i8, v: i32) {
    unsafe {
        printf(c"%s = %d\n".as_ptr(), label, v);
    }
}

fn ord(o: Ordering) -> i32 {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Point {
    x: i32,
    y: u8,
}

#[derive(PartialEq, PartialOrd)]
struct Pair<T>(T, bool);

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Empty;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Shape {
    Dot,
    Line(i32, i32),
    Box { corner: Point, side: u8 },
    Nothing,
}

// Ordered by discriminant, not by declaration.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i8)]
enum Rank {
    High = 10,
    Low = -3,
    Middle = 4,
}

const BASE: isize = 2;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Counted {
    A = BASE * 3,
    B,
    C = 1,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Tree<T> {
    Leaf(T),
    Node { left: T, right: T },
}

struct Loud(i32);

impl Drop for Loud {
    fn drop(&mut self) {
        show(c"drop".as_ptr(), self.0);
    }
}

impl PartialEq for Loud {
    fn eq(&self, other: &Loud) -> bool {
        self.0 == other.0
    }
}

impl Eq for Loud {}

impl PartialOrd for Loud {
    fn partial_cmp(&self, other: &Loud) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Reversed, to show `<` and `max` go through these impls.
impl Ord for Loud {
    fn cmp(&self, other: &Loud) -> Ordering {
        other.0.cmp(&self.0)
    }
}

fn largest<T: Ord>(a: T, b: T, c: T) -> T {
    a.max(b).max(c)
}

fn sorted<T: PartialOrd>(a: &T, b: &T, c: &T) -> bool {
    a <= b && b <= c
}

fn compare<T: Ord>(a: &T, b: &T) -> i32 {
    ord(a.cmp(b))
}

fn integers(a: i32, b: i32, u: u8, w: i64) {
    show(c"a.cmp(&b)".as_ptr(), ord(a.cmp(&b)));
    show(c"b.cmp(&a)".as_ptr(), ord(b.cmp(&a)));
    show(c"a.cmp(&a)".as_ptr(), ord(a.cmp(&a)));
    show(c"a.max(b)".as_ptr(), a.max(b));
    show(c"a.min(b)".as_ptr(), a.min(b));
    show(c"i32::max(a, b)".as_ptr(), i32::max(a, b));
    show(c"Ord::min(a, b)".as_ptr(), Ord::min(a, b));
    show(c"cmp::max(a, b)".as_ptr(), cmp::max(a, b));
    show(c"cmp::min(a, b)".as_ptr(), cmp::min(a, b));
    show(c"u.clamp(3, 9)".as_ptr(), u.clamp(3, 9) as i32);
    show(c"w.clamp(-5, 5)".as_ptr(), w.clamp(-5, 5) as i32);
    show(c"(-w).clamp(-5, 5)".as_ptr(), (-w).clamp(-5, 5) as i32);
    show(c"7.max(a)".as_ptr(), 7.max(a));
    show(c"a.partial_cmp(&b)".as_ptr(), ord(a.partial_cmp(&b).unwrap()));
    show(c"a.lt(&b)".as_ptr(), a.lt(&b) as i32);
    show(c"a.ge(&b)".as_ptr(), a.ge(&b) as i32);
    show(c"&a < &b".as_ptr(), (&a < &b) as i32);
    show(c"sorted(&a, &b, &b)".as_ptr(), sorted(&a, &b, &b) as i32);
    show(c"largest(a, b, 4)".as_ptr(), largest(a, b, 4));
    show(c"compare(&u, &3)".as_ptr(), compare(&u, &3));
}

fn references(a: &mut i32, b: &mut i32) {
    show(c"a < b".as_ptr(), (a < b) as i32);
    show(c"a >= b".as_ptr(), (a >= b) as i32);
    *a += 10;
    show(c"a > b".as_ptr(), (a > b) as i32);
    show(c"(*a).cmp(b)".as_ptr(), ord((*a).cmp(b)));
}

fn bools(t: bool, f: bool) {
    show(c"f < t".as_ptr(), (f < t) as i32);
    show(c"t.cmp(&f)".as_ptr(), ord(t.cmp(&f)));
    show(c"t.max(f)".as_ptr(), t.max(f) as i32);
    show(c"t.min(f)".as_ptr(), t.min(f) as i32);
    show(c"().cmp(&())".as_ptr(), ord(().cmp(&())));
}

fn options(a: Option<i32>, b: Option<i32>, none: Option<i32>) {
    show(c"a < b".as_ptr(), (a < b) as i32);
    show(c"none < a".as_ptr(), (none < a) as i32);
    show(c"a.cmp(&none)".as_ptr(), ord(a.cmp(&none)));
    show(c"none.cmp(&none)".as_ptr(), ord(none.cmp(&none)));
    show(c"a.max(none)".as_ptr(), a.max(none).unwrap());
    let ok: Result<i32, u8> = Ok(5);
    let err: Result<i32, u8> = Err(0);
    show(c"ok < err".as_ptr(), (ok < err) as i32);
    show(c"err.cmp(&Err(1))".as_ptr(), ord(err.cmp(&Err(1))));
    show(c"ok.cmp(&Ok(4))".as_ptr(), ord(ok.cmp(&Ok(4))));
}

fn orderings(o: Ordering) {
    show(c"o.is_lt()".as_ptr(), o.is_lt() as i32);
    show(c"o.is_le()".as_ptr(), o.is_le() as i32);
    show(c"o.is_gt()".as_ptr(), o.is_gt() as i32);
    show(c"o.is_ge()".as_ptr(), o.is_ge() as i32);
    show(c"o.is_eq()".as_ptr(), o.is_eq() as i32);
    show(c"o.is_ne()".as_ptr(), o.is_ne() as i32);
    show(c"o.reverse()".as_ptr(), ord(o.reverse()));
    show(c"Equal.then(o)".as_ptr(), ord(Ordering::Equal.then(o)));
    show(c"o.then(Greater)".as_ptr(), ord(o.then(Ordering::Greater)));
    show(c"o as i32".as_ptr(), o as i32);
    show(c"o < Equal".as_ptr(), (o < Ordering::Equal) as i32);
    show(c"o == Less".as_ptr(), (o == Ordering::Less) as i32);
    show(c"o.cmp(&Greater)".as_ptr(), ord(o.cmp(&Ordering::Greater)));
}

fn derived(x: i32, y: u8) {
    let p = Point { x, y };
    let q = Point { x, y: y + 1 };
    let r = Point { x: x - 1, y: 200 };
    show(c"p.cmp(&q)".as_ptr(), ord(p.cmp(&q)));
    show(c"p.cmp(&r)".as_ptr(), ord(p.cmp(&r)));
    show(c"p < q".as_ptr(), (p < q) as i32);
    show(c"r.max(p).y".as_ptr(), r.max(p).y as i32);
    show(c"Pair(x, true) < Pair(x, false)".as_ptr(), (Pair(x, true) < Pair(x, false)) as i32);
    show(c"Pair(p, false) > Pair(r, true)".as_ptr(), (Pair(p, false) > Pair(r, true)) as i32);
    show(c"Empty.cmp(&Empty)".as_ptr(), ord(Empty.cmp(&Empty)));
    let line = Shape::Line(x, 2);
    show(c"line.cmp(&Dot)".as_ptr(), ord(line.cmp(&Shape::Dot)));
    show(c"line.cmp(&line)".as_ptr(), ord(line.cmp(&line)));
    show(c"line.cmp(&Box)".as_ptr(), ord(line.cmp(&Shape::Box { corner: p, side: y })));
    show(c"line.cmp(&Nothing)".as_ptr(), ord(line.cmp(&Shape::Nothing)));
    show(c"Line(x, 3) > line".as_ptr(), (Shape::Line(x, 3) > line) as i32);
    let big = Shape::Box { corner: q, side: 0 };
    show(c"big > Box(p)".as_ptr(), (big > Shape::Box { corner: p, side: 9 }) as i32);
    show(c"High > Middle".as_ptr(), (Rank::High > Rank::Middle) as i32);
    show(c"Low.cmp(&Middle)".as_ptr(), ord(Rank::Low.cmp(&Rank::Middle)));
    show(c"Middle.clamp(Low, High)".as_ptr(), Rank::Middle.clamp(Rank::Low, Rank::High) as i32);
    show(c"B > A".as_ptr(), (Counted::B > Counted::A) as i32);
    show(c"C < A".as_ptr(), (Counted::C < Counted::A) as i32);
    let leaf = Tree::Leaf(x);
    let node = Tree::Node { left: x, right: 0 };
    show(c"leaf < node".as_ptr(), (leaf < node) as i32);
    show(c"node.cmp(Node)".as_ptr(), ord(node.cmp(&Tree::Node { left: x, right: -1 })));
}

fn destructors(a: i32, b: i32) {
    let m = Loud(a).max(Loud(b));
    show(c"Loud max".as_ptr(), m.0);
    let n = cmp::min(Loud(a), Loud(b));
    show(c"Loud min".as_ptr(), n.0);
    show(c"Loud(a) < Loud(b)".as_ptr(), (Loud(a) < Loud(b)) as i32);
    let c = Loud(5).clamp(Loud(b), Loud(a));
    show(c"Loud clamp".as_ptr(), c.0);
}

// `clamp` panics if its bounds are the wrong way round.
fn bad_clamp(low: i32, high: i32) {
    show(c"bad clamp".as_ptr(), 5.clamp(high, low));
}

fn main() {
    integers(3, 8, 12, 9);
    references(&mut 4, &mut 6);
    bools(true, false);
    options(Some(2), Some(7), None);
    orderings(Ordering::Less);
    orderings(Ordering::Equal);
    orderings(Ordering::Greater);
    derived(5, 9);
    destructors(1, 9);
    bad_clamp(1, 9);
}
