// Methods of trait `impl` blocks whose signatures name the block's
// associated types (`Self::Out`, `<Self as Tr>::Out`), in blocks for
// references and with bounds on their parameters.

use core::ops::{Add, Mul, Neg};

struct Bag<T> {
    items: [T; 2],
}

struct It<'a, T> {
    bag: &'a Bag<T>,
    i: usize,
}

impl<'a, T: Copy> Iterator for It<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.i < 2 {
            self.i += 1;
            Some(&self.bag.items[self.i - 1])
        } else {
            None
        }
    }
}

// The bound on `T` holds in the block, though not for every `T`.
impl<'a, T: Copy> IntoIterator for &'a Bag<T> {
    type Item = &'a T;
    type IntoIter = It<'a, T>;

    fn into_iter(self) -> It<'a, T> {
        It { bag: self, i: 0 }
    }
}

struct Pair {
    items: [i32; 2],
}

struct PairIt<'a> {
    pair: &'a Pair,
    i: usize,
}

impl<'a> Iterator for PairIt<'a> {
    type Item = &'a i32;

    fn next(&mut self) -> Option<&'a i32> {
        if self.i < 2 {
            self.i += 1;
            Some(&self.pair.items[self.i - 1])
        } else {
            None
        }
    }
}

impl<'a> IntoIterator for &'a Pair {
    type Item = &'a i32;
    type IntoIter = PairIt<'a>;

    fn into_iter(self) -> Self::IntoIter {
        PairIt { pair: self, i: 0 }
    }
}

struct Wrap<T> {
    v: [T; 2],
}

struct WrapIt<'a, T> {
    w: &'a Wrap<T>,
    i: usize,
}

impl<'a, T> Iterator for WrapIt<'a, T>
where
    T: Copy,
{
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.i < 2 {
            self.i += 1;
            Some(self.w.v[self.i - 1])
        } else {
            None
        }
    }
}

impl<'a, T> IntoIterator for &'a Wrap<T>
where
    T: Copy,
{
    type Item = T;
    type IntoIter = WrapIt<'a, T>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        WrapIt { w: self, i: 0 }
    }
}

fn total<T: Copy + Add<Output = T>>(w: &Wrap<T>, zero: T) -> T {
    let mut t = zero;
    for x in w {
        t = t + x;
    }
    t
}

fn sum_bag<T: Copy>(b: &Bag<T>, f: fn(T) -> i64) -> i64 {
    let mut t = 0;
    for x in b {
        t += f(*x);
    }
    t
}

#[derive(Clone, Copy)]
struct V<T> {
    x: T,
    y: T,
}

impl<'a, 'b, T: Copy + Add<Output = T>> Add<&'b V<T>> for &'a V<T> {
    type Output = V<T>;

    fn add(self, o: &'b V<T>) -> Self::Output {
        V {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }
}

impl<'a, 'b, T> Mul<&'b V<T>> for &'a V<T>
where
    T: Copy + Mul<Output = T> + Add<Output = T>,
{
    type Output = T;

    fn mul(self, o: &'b V<T>) -> <Self as Mul<&'b V<T>>>::Output {
        self.x * o.x + self.y * o.y
    }
}

impl<'a, T: Copy + Neg<Output = T>> Neg for &'a V<T> {
    type Output = V<T>;

    fn neg(self) -> V<T> {
        V {
            x: -self.x,
            y: -self.y,
        }
    }
}

fn add_refs<T: Copy + Add<Output = T>>(a: &V<T>, b: &V<T>) -> V<T> {
    a + b
}

trait Shape {
    type Unit: Copy;

    fn area(&self) -> Self::Unit;

    fn twice(&self, u: Self::Unit) -> (Self::Unit, Self::Unit) {
        (u, u)
    }

    fn sides(&self) -> i32 {
        4
    }
}

struct Square(i32);

impl Shape for Square {
    type Unit = i32;

    fn area(&self) -> Self::Unit {
        self.0 * self.0
    }

    fn twice(&self, u: <Self as Shape>::Unit) -> (i32, Self::Unit) {
        (u, u * 2)
    }
}

impl<'a, S: Shape> Shape for &'a S {
    type Unit = S::Unit;

    fn area(&self) -> Self::Unit {
        (**self).area()
    }
}

trait Visit {
    type Out;

    fn visit<F: Fn(i32) -> i32>(&self, f: F) -> Self::Out;

    fn visit2<G>(&self, g: G) -> i32
    where
        G: Fn(i32) -> i32;
}

struct W(i32);

impl Visit for W {
    type Out = i32;

    fn visit<H: Fn(i32) -> i32>(&self, h: H) -> i32 {
        h(self.0)
    }

    fn visit2<G>(&self, g: G) -> i32
    where
        G: Fn(i32) -> i32,
    {
        g(self.0) + 1
    }
}

impl<'a> Visit for &'a W {
    type Out = &'a W;

    fn visit<F: Fn(i32) -> i32>(&self, _f: F) -> Self::Out {
        *self
    }

    fn visit2<G: Fn(i32) -> i32>(&self, g: G) -> i32 {
        g(self.0) * 2
    }
}

fn widen(x: u8) -> i64 {
    x as i64
}

fn main() {
    let b = Bag { items: [3, 4] };
    let mut t = 0;
    for x in &b {
        t += *x;
    }
    let p = Pair { items: [5, 6] };
    for x in &p {
        t += *x;
    }
    let w = Wrap { v: [1u8, 2] };
    let a = V { x: 1, y: 2 };
    let c = V { x: 3, y: 4 };
    let sum = &a + &c;
    let dot = &a * &c;
    let neg = -&a;
    let s = Square(3);
    let r = &s;
    let (u, v) = s.twice(2);
    let k = W(5);
    let kr = &k;
    let _ = (
        t,
        total(&w, 0),
        sum_bag(&Bag { items: [1u8, 2] }, widen),
        sum.x,
        dot,
        neg.y,
        add_refs(&a, &c).x,
        r.area(),
        r.sides(),
        u + v,
        k.visit(|x| x + 1),
        k.visit2(|x| x * 3),
        <&W as Visit>::visit(&kr, |x| x).0,
        <&W as Visit>::visit2(&kr, |x| x - 1),
    );
}
