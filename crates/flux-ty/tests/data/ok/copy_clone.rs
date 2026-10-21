// `Clone`, `Copy`, `PartialEq` and `Eq`, written by hand and derived:
// `clone` called as a method (through autoref and autoderef) and by path,
// `Copy` implying `Clone`, bounds and `where` clauses that need them,
// derived impls of generic structs used through those bounds, and `==` and
// `!=` on derived types and references to them.

#[derive(Clone, Copy, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, PartialEq)]
struct Pair<T>(T, T);

#[derive(Clone, PartialEq, Eq)]
struct Named {
    id: u8,
    at: Point,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Unit;

#[derive(Clone, PartialEq)]
struct Empty {}

// Several parameters, each bounded by the derive.
#[derive(Clone, PartialEq)]
struct Two<A, B> {
    a: A,
    b: B,
}

// By hand: `Clone` with and without `Copy`.
struct Handle(u32);

impl Clone for Handle {
    fn clone(&self) -> Handle {
        Handle(self.0 + 1)
    }
}

struct Id(u64);

impl Clone for Id {
    fn clone(&self) -> Id {
        *self
    }
}

impl Copy for Id {}

// By hand, for a generic struct: bounds on the parameter.
struct Wrap<T>(T);

impl<T: Clone> Clone for Wrap<T> {
    fn clone(&self) -> Wrap<T> {
        Wrap(self.0.clone())
    }
}

impl<T: Copy> Copy for Wrap<T> {}

// `clone` as a method, through autoref on a value and on a reference, and
// by path.
fn clones(p: Point, r: &Point, h: &Handle) -> (Point, Point, Handle, Handle) {
    let a = p.clone();
    let b = r.clone();
    let c = h.clone();
    let d = Clone::clone(h);
    let _e = Point::clone(r);
    let _f = <Handle as Clone>::clone(h);
    (a, b, c, d)
}

// Copying out of references.
fn copies(r: &Point, m: &mut Id, w: &Wrap<i32>) -> (Point, Id, i32) {
    let p = *r;
    let i = *m;
    (p, i, w.0)
}

// `T: Copy` implies `T: Clone`.
fn dup<T: Copy>(t: T) -> (T, T) {
    (t.clone(), t)
}

fn dup_ref<T: Copy>(t: &T) -> (T, T) {
    (*t, t.clone())
}

fn clone_ref<T: Clone>(t: &T) -> T {
    t.clone()
}

fn dup_where<T>(t: T) -> (T, T)
where
    T: Copy,
{
    (t, t)
}

// Bounds on an impl block's parameters.
impl<T: Copy> Pair<T> {
    fn first(&self) -> T {
        self.0
    }
}

impl<T> Pair<T>
where
    T: Clone,
{
    fn second(&self) -> T {
        self.1.clone()
    }
}

// Derived impls of generic structs, through bounds.
fn pairs<T: Copy + PartialEq>(a: Pair<T>, b: &Pair<T>) -> (Pair<T>, bool, bool) {
    let c = a.clone();
    (a, c == *b, a != *b)
}

fn twos(t: &Two<i32, Unit>) -> Two<i32, Unit> {
    t.clone()
}

// A supertrait that implies `Copy`.
trait Marker: Copy {}

impl Marker for Point {}

fn via_super<T: Marker>(t: &T) -> (T, T) {
    (t.clone(), *t)
}

// Associated types bounded by `Copy`.
trait Source {
    type Item: Copy;

    fn get(&self) -> Self::Item;
}

fn twice<S: Source>(s: &S) -> (S::Item, S::Item) {
    let item = s.get();
    (item, item)
}

// `==` and `!=` on derived types, on references to them, and through
// `PartialEq` and `Eq` bounds.
fn same<T: Eq>(a: &T, b: &T) -> bool {
    a == b
}

fn compare(p: Point, q: &Point, n: &Named, e: Empty) -> (bool, bool, bool, bool, bool) {
    let a = p == *q;
    let b = &p != q;
    let c = *n == n.clone();
    let d = same(n, n) && same(&Unit, &Unit);
    let f = e == Empty {} && Pair(1u8, 2) != Pair(2, 1);
    (a, b, c, d, f)
}

fn explicit(p: &Point, q: &Point) -> (bool, bool) {
    (p.eq(q), PartialEq::ne(p, q))
}

// Integers, `bool`, `()` and tuples clone through `core`'s impls.
fn primitives(t: &(i32, bool, ())) -> ((i32, bool, ()), u8) {
    (t.clone(), 7u8.clone())
}

// `clone_from`.
fn reset(p: &mut Point, q: &Point) {
    p.clone_from(q);
}

fn main() {}
