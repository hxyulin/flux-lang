//! Trait `impl` blocks for references, with bounds on their parameters
//! and methods naming the block's associated types (`Self::IntoIter`,
//! `<Self as Tr>::Out`); operators on references; references to
//! references dereferenced where a reference is expected; and
//! references passed to a C variadic function.

use core::ops::{Add, Mul, Neg};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

struct Bag<T> {
    items: [T; 3],
}

struct It<'a, T> {
    bag: &'a Bag<T>,
    i: usize,
}

impl<'a, T: Copy> Iterator for It<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.i < 3 {
            self.i += 1;
            Some(&self.bag.items[self.i - 1])
        } else {
            None
        }
    }
}

impl<'a, T: Copy> IntoIterator for &'a Bag<T> {
    type Item = &'a T;
    type IntoIter = It<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
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

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        PairIt { pair: self, i: 0 }
    }
}

fn sum_bag<T: Copy>(b: &Bag<T>, widen: fn(T) -> i64) -> i64 {
    let mut t = 0;
    for x in b {
        t += widen(*x);
    }
    t
}

fn widen_u8(x: u8) -> i64 {
    x as i64
}

fn widen_i32(x: i32) -> i64 {
    x as i64
}

fn largest(p: &Pair) -> &i32 {
    let mut best = &p.items[0];
    for x in p {
        if *x > *best {
            best = x;
        }
    }
    best
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

struct Figure {
    sides: i32,
}

fn sides(f: &Figure) -> i32 {
    f.sides
}

fn bump(f: &mut Figure) {
    f.sides += 1;
}

fn pick<'a>(r: &'a &'a Figure) -> &'a Figure {
    r
}

fn through<'a, 'b>(r: &'a mut &'b mut Figure) -> &'a Figure {
    r
}

fn main() {
    show(1, sum_bag(&Bag { items: [1u8, 2, 3] }, widen_u8));
    show(2, sum_bag(&Bag { items: [10, 20, 30] }, widen_i32));
    let p = Pair { items: [4, 9] };
    show(3, *largest(&p) as i64);

    let a = V { x: 1, y: 2 };
    let b = V { x: 3, y: 4 };
    let s = &a + &b;
    show(4, (s.x * 10 + s.y) as i64);
    show(5, (&a * &b) as i64);
    let n = -&b;
    show(6, (n.x + n.y) as i64);
    show(7, add_refs(&a, &b).y as i64);

    let sq = Square(3);
    let r = &sq;
    let (u, v) = sq.twice(5);
    show(8, (sq.area() * 100 + r.area()) as i64);
    show(9, (u * 10 + v) as i64);
    show(10, Shape::sides(&r) as i64);

    let f = Figure { sides: 3 };
    let g = Figure { sides: 5 };
    let figures = [&f, &g];
    let mut t = 0;
    for x in &figures {
        t += sides(x);
    }
    show(11, t as i64);
    let rf = &f;
    let rrf = &rf;
    let by_closure = |x: &Figure| x.sides * 2;
    let captured = || sides(&rrf);
    show(12, (by_closure(&rrf) * 10 + captured()) as i64);
    let mut h = Figure { sides: 7 };
    {
        let mut m = &mut h;
        bump(&mut m);
        let mm = &mut m;
        bump(mm);
        show(13, through(mm).sides as i64);
    }
    show(14, pick(&&h).sides as i64);

    let text = [104u8, 105, 0];
    let mut more = [33u8, 33, 0];
    more[1] = 63;
    unsafe {
        printf(c"%s%s\n".as_ptr(), &text, &mut more);
    }
}
