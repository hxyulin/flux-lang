// Moves and copies: values of types that implement `Copy` (by hand or
// derived, integers, `bool`, tuples of them, shared references) are
// copied, so they stay usable; generic code copies `T: Copy` values (from
// bounds, `where` clauses, impl blocks, supertraits and associated type
// bounds); derived `Copy` on a generic struct copies it when its argument
// is `Copy`; and `clone()` keeps a value that is not `Copy` usable.

#[derive(Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy)]
struct Pair<T>(T, T);

struct Id(u64);

impl Clone for Id {
    fn clone(&self) -> Id {
        *self
    }
}

impl Copy for Id {}

#[derive(Clone)]
struct Owned(i32);

struct Line {
    from: Point,
    to: Point,
    tag: Owned,
}

fn take<T>(_t: T) {}

// A `Copy` struct used after being passed, assigned and returned.
fn copied(p: Point, i: Id) -> (Point, Point, Id, Id) {
    take(p);
    let q = p;
    take(i);
    (p, q, i, i)
}

impl Point {
    fn shifted(self, by: i32) -> Point {
        Point {
            x: self.x + by,
            y: self.y + by,
        }
    }
}

// Methods taking `self` by value, on a `Copy` value and through a
// reference to one.
fn by_value(p: Point, r: &Point) -> i32 {
    let a = p.shifted(1);
    let b = p.shifted(2);
    let c = r.shifted(3);
    a.x + b.x + c.x + r.y
}

// Copying out of references, and copying a `Copy` field out of a struct
// that is not `Copy`.
fn through_refs(r: &Point, m: &mut Point, l: &Line) -> (Point, Point, Point, i32) {
    let a = *r;
    let b = *m;
    let c = l.from;
    (a, b, c, l.tag.0)
}

// Partial moves leave the other fields usable, and `Copy` fields can be
// used after their struct was partially moved.
fn partial(l: Line) -> (Owned, Point, Point, Point) {
    let tag = l.tag;
    let from = l.from;
    (tag, from, l.from, l.to)
}

// Generic code copies `T: Copy`, however the bound is written.
fn twice<T: Copy>(t: T) -> (T, T) {
    take(t);
    (t, t)
}

fn twice_where<T>(t: T) -> (T, T)
where
    T: Copy,
{
    (t, t)
}

impl<T: Copy> Pair<T> {
    fn swap(self) -> Pair<T> {
        Pair(self.1, self.0)
    }

    fn first(&self) -> T {
        self.0
    }
}

trait Marker: Copy {}

impl Marker for Point {}

fn via_super<T: Marker>(t: &T) -> (T, T) {
    let a = *t;
    (a, a)
}

trait Source {
    type Item: Copy;

    fn get(&self) -> Self::Item;
}

fn item_twice<S: Source>(s: &S) -> (S::Item, S::Item) {
    let item = s.get();
    (item, item)
}

// Derived `Copy` on a generic struct: copied when its argument is.
fn generic_copy(p: Pair<i32>, q: Pair<Point>) -> (Pair<i32>, Pair<i32>, Pair<Point>, Pair<Point>) {
    let r = p.swap();
    take(q);
    (p, r, q, q)
}

fn generic_copy_param<T: Copy>(p: Pair<T>) -> (T, T, Pair<T>) {
    (p.first(), p.swap().0, p)
}

// Tuples of `Copy` types and shared references are `Copy`.
fn tuples(t: (i32, bool, Point), r: &Owned) -> (i32, i32) {
    let a = t;
    let b = t;
    take(r);
    take(r);
    let s = r;
    (a.0 + b.2.x, s.0 + r.0)
}

// `clone()` instead of moving.
fn clones(o: Owned, l: &Line) -> (Owned, Owned, Owned) {
    let a = o.clone();
    take(o.clone());
    let b = l.tag.clone();
    (o, a, b)
}

fn clones_param<T: Clone>(t: T) -> (T, T) {
    let u = t.clone();
    (t, u)
}

// A moved-from struct can be assigned again.
fn reassign(mut o: Owned) -> Owned {
    take(o);
    o = Owned(1);
    o
}

fn main() {}
