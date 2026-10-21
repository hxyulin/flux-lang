//! Associated types and operators: a trait's associated type defined
//! by each `impl` block and used through `Self::Item`, `C::Item` and
//! `<Pair as Container>::Item`, in provided methods and generic code,
//! with bounds that bind it (`C: Container<Item = i32>`); a trait
//! parameter's default (`Convert<T = i32>`), implemented twice; bounds
//! on associated types (with a binding, and one bounded by its own
//! trait) and on `C::Item` in a `where` clause; the program's impls
//! for integers on the left of operators; and the
//! operators on user types through `core::ops` and `PartialEq`: binary,
//! compound assignment, unary, `==` and `!=` (also on references), with
//! generic code using them through bounds, instantiated with structs and
//! integers.

use core::ops::{Add, AddAssign, Mul, Neg, Not, Sub};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

trait Container {
    type Item;
    fn get(&self, i: i32) -> Self::Item;
    fn first(&self) -> Self::Item {
        self.get(0)
    }
}

struct Pair(i32, i32);
struct Flags(bool, bool);

impl Container for Pair {
    type Item = i32;
    fn get(&self, i: i32) -> i32 {
        if i == 0 { self.0 } else { self.1 }
    }
}

impl Container for Flags {
    type Item = bool;
    fn get(&self, i: i32) -> Self::Item {
        if i == 0 { self.0 } else { self.1 }
    }
}

struct Wrap<C>(C);

impl<C: Container> Container for Wrap<C> {
    type Item = C::Item;
    fn get(&self, i: i32) -> C::Item {
        self.0.get(i + 1)
    }
}

fn second<C: Container>(c: &C) -> C::Item {
    c.get(1)
}

fn sum<C>(c: &C) -> i32
where
    C: Container<Item = i32>,
{
    c.get(0) + c.get(1)
}

trait Convert<T = i32> {
    fn convert(&self) -> T;
}

impl Convert for Pair {
    fn convert(&self) -> i32 {
        self.0 * 10 + self.1
    }
}

impl Convert<bool> for Pair {
    fn convert(&self) -> bool {
        self.0 < self.1
    }
}


struct V(i32, i32);

impl Add for V {
    type Output = V;
    fn add(self, rhs: V) -> V {
        V(self.0 + rhs.0, self.1 + rhs.1)
    }
}

impl Sub for V {
    type Output = V;
    fn sub(self, rhs: V) -> V {
        V(self.0 - rhs.0, self.1 - rhs.1)
    }
}

impl Mul<i32> for V {
    type Output = V;
    fn mul(self, k: i32) -> V {
        V(self.0 * k, self.1 * k)
    }
}

impl Neg for V {
    type Output = V;
    fn neg(self) -> V {
        V(-self.0, -self.1)
    }
}

impl AddAssign for V {
    fn add_assign(&mut self, rhs: V) {
        self.0 += rhs.0;
        self.1 += rhs.1;
    }
}

impl PartialEq for V {
    fn eq(&self, other: &V) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
}

struct Flag(bool);

impl Not for Flag {
    type Output = bool;
    fn not(self) -> bool {
        !self.0
    }
}

fn sum3<T: Add<Output = T>>(a: T, b: T, c: T) -> T {
    a + b + c
}

fn same<T: PartialEq>(a: &T, b: &T) -> bool {
    a == b && !(a != b)
}

fn both<T>(x: T, y: T) -> T::Output
where
    T: Add,
{
    x + y
}

// Bounds on associated types, with a binding generic code relies on, a
// `where` clause on `T::Item`, and an associated type bounded by its own
// trait.
trait Num {
    type N: PartialEq + Add<i32, Output = i32>;
    fn n(&self) -> Self::N;
}

impl Num for V {
    type N = i32;
    fn n(&self) -> i32 {
        self.0 + self.1
    }
}

fn add_n<T: Num>(a: &T, b: &T) -> i32 {
    (a.n() + 100) * 10 + (a.n() == b.n()) as i32
}

fn firsts<C: Container>(a: &C, b: &C) -> C::Item
where
    C::Item: Add<Output = C::Item>,
{
    a.first() + b.first()
}

trait Chain {
    type Next: Chain;
    fn depth(&self) -> i32;
    fn next(&self) -> Self::Next;
}

impl Chain for Pair {
    type Next = V;
    fn depth(&self) -> i32 {
        self.0
    }
    fn next(&self) -> V {
        V(self.0 + 1, self.1)
    }
}

impl Chain for V {
    type Next = Pair;
    fn depth(&self) -> i32 {
        self.0 * 100
    }
    fn next(&self) -> Pair {
        Pair(self.0 + 1, self.1)
    }
}

fn third<C: Chain>(c: &C) -> i32 {
    c.next().next().depth()
}

// An integer on the left, through the program's impls.
struct Tens(i32);

impl Add<Tens> for i32 {
    type Output = i64;
    fn add(self, rhs: Tens) -> i64 {
        (self + rhs.0) as i64 * 10
    }
}

impl PartialEq<Tens> for u8 {
    fn eq(&self, other: &Tens) -> bool {
        *self as i32 == other.0
    }
}

impl AddAssign<Tens> for i32 {
    fn add_assign(&mut self, rhs: Tens) {
        *self += rhs.0 * 2;
    }
}

fn main() {
    let p = Pair(3, 4);
    let f = Flags(true, false);
    let w = Wrap(Pair(5, 6));
    let x: <Pair as Container>::Item = p.first();
    let b: bool = second(&f);
    let n: i32 = Convert::convert(&p);
    let c: bool = p.convert();
    unsafe {
        printf(c"%d %d %d %d\n".as_ptr(), x, second(&p), sum(&p), b as i32);
        printf(c"%d %d %d\n".as_ptr(), w.first(), sum(&w), Wrap(Pair(1, 9)).get(0));
        printf(c"%d %d\n".as_ptr(), n, c as i32);
    }
    let a = V(1, 2) + V(3, 4);
    let b = -(a - V(1, 1)) * 3;
    let mut c = V(10, 20);
    c += V(1, 1);
    let d = sum3(V(1, 1), V(2, 2), V(3, 3));
    let n = sum3(1, 2, 3) + sum3(10u8, 20, 30) as i32;
    unsafe {
        printf(c"%d %d %d %d\n".as_ptr(), b.0, b.1, c.0, c.1);
        printf(c"%d %d %d\n".as_ptr(), d.0, d.1, n);
        printf(
            c"%d %d %d %d\n".as_ptr(),
            (V(1, 2) == V(1, 2)) as i32,
            (V(1, 2) != V(1, 2)) as i32,
            same(&V(1, 2), &V(2, 1)) as i32,
            same(&5u64, &5u64) as i32,
        );
        printf(c"%d %d\n".as_ptr(), (!Flag(false)) as i32, both(20i32, 1));
        printf(
            c"%d %d %d %d\n".as_ptr(),
            add_n(&V(1, 2), &V(2, 1)),
            firsts(&Pair(3, 4), &Pair(5, 6)),
            third(&Pair(7, 0)),
            third(&V(8, 0)),
        );
    }
    let mut t = 5;
    t += Tens(4);
    unsafe {
        printf(
            c"%lld %d %d %d\n".as_ptr(),
            1 + Tens(2),
            t,
            (3u8 == Tens(3)) as i32,
            (3u8 != Tens(4)) as i32,
        );
    }
}
