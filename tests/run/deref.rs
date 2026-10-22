//! `Deref` and `DerefMut`: `*w` on smart-pointer-like structs, fields
//! and methods found through them (autoderef, also several levels deep
//! and mixed with references), indexing and slice methods through a
//! `Deref` to a slice, deref coercion of `&W` to `&W::Target` at calls
//! and `let`s, `DerefMut` chosen where a place is assigned, borrowed
//! mutably or a `&mut self` method is called (counted by the impl),
//! generic code bounded by `Deref`, matching on `*w`, and closures that
//! capture a value used through its `Deref`.

use core::ops::{Deref, DerefMut};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Circle(i32),
    Square(i32),
}

struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn sum(&self) -> i32 {
        self.x + self.y
    }

    fn bump(&mut self) {
        self.x += 1;
    }
}

struct Wrapper<T> {
    inner: T,
    reads: i32,
}

impl<T> Deref for Wrapper<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> DerefMut for Wrapper<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

/// Counts its mutable dereferences.
struct Guard<'a, T> {
    value: &'a mut T,
    uses: u32,
}

impl<'a, T> Deref for Guard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

impl<'a, T> DerefMut for Guard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.uses += 1;
        self.value
    }
}

struct Arr {
    data: [i32; 4],
}

impl Deref for Arr {
    type Target = [i32];
    fn deref(&self) -> &[i32] {
        &self.data
    }
}

impl DerefMut for Arr {
    fn deref_mut(&mut self) -> &mut [i32] {
        &mut self.data
    }
}

fn takes_point(p: &Point) -> i32 {
    p.x * 10 + p.y
}

fn takes_slice(s: &[i32]) -> usize {
    s.len()
}

fn takes_mut_slice(s: &mut [i32]) {
    s[0] = 100;
}

fn bump(x: &mut i32) {
    *x += 1;
}

fn area(s: &Shape) -> i32 {
    match s {
        Shape::Circle(r) => 3 * r * r,
        Shape::Square(a) => a * a,
    }
}

fn total<D: Deref<Target = [i32]>>(d: &D) -> i32 {
    d.iter().sum()
}

fn first<D: Deref>(d: D) -> D::Target
where
    D::Target: Copy,
{
    *d
}

fn main() {
    let mut w = Wrapper {
        inner: Point { x: 1, y: 2 },
        reads: 0,
    };
    show(w.x as i64);
    show(w.sum() as i64);
    w.bump();
    show(w.x as i64);
    w.y = 40;
    show((*w).y as i64);
    show(takes_point(&w) as i64);
    (*w).x = 7;
    show(w.reads as i64 + w.x as i64);

    let mut a = Arr { data: [1, 2, 3, 4] };
    show(a.len() as i64);
    show(a[2] as i64);
    a[1] = 20;
    show(a.iter().sum::<i32>() as i64);
    show(takes_slice(&a) as i64);
    takes_mut_slice(&mut a);
    show(a[0] as i64);
    show(a[1..3].len() as i64);
    show(total(&a) as i64);

    let ww = Wrapper {
        inner: Wrapper {
            inner: 5i32,
            reads: 0,
        },
        reads: 0,
    };
    show(**ww as i64);
    let r: &i32 = &ww;
    show(*r as i64);

    let mut n = 5;
    {
        let mut g = Guard {
            value: &mut n,
            uses: 0,
        };
        *g += 10;
        bump(&mut g);
        bump(&mut *g);
        show(*g as i64);
        show(g.uses as i64);
    }
    show(n as i64);

    let b = Wrapper {
        inner: Shape::Square(4),
        reads: 0,
    };
    show(area(&b) as i64);
    match *b {
        Shape::Circle(r) => show(r as i64),
        Shape::Square(a) => show(a as i64 * 100),
    }
    if let Shape::Square(a) = *b {
        show(a as i64);
    }
    show((*b == Shape::Square(4)) as i64);
    show(first(&7) as i64);
    show(first(Wrapper { inner: 9u8, reads: 0 }) as i64);

    let mut nested = Wrapper {
        inner: Wrapper {
            inner: [1, 2, 3],
            reads: 0,
        },
        reads: 0,
    };
    nested[2] = 30;
    nested.inner.inner[0] = 10;
    for x in nested.iter_mut() {
        *x += 1;
    }
    show(nested.iter().sum::<i32>() as i64);

    let mut c = Wrapper { inner: 0i32, reads: 0 };
    let mut add = |k: i32| *c += k;
    add(3);
    add(4);
    show(*c as i64);
    let f = || w.x + 1;
    show(f() as i64);
}
