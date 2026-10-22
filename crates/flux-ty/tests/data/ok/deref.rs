use core::ops::{Deref, DerefMut};

struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn sum(&self) -> i32 {
        self.x + self.y
    }
}

struct Wrapper<T> {
    inner: T,
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

struct Guard<'a, T> {
    value: &'a mut T,
}

impl<'a, T> Deref for Guard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

struct Arr {
    data: [i32; 2],
}

impl Deref for Arr {
    type Target = [i32];
    fn deref(&self) -> &[i32] {
        &self.data
    }
}

fn takes_point(p: &Point) -> i32 {
    p.x
}

fn takes_slice(s: &mut [u8]) -> usize {
    s.len()
}

fn total<D: Deref<Target = [i32]>>(d: &D) -> i32 {
    d.iter().sum()
}

fn main() {
    let mut w = Wrapper {
        inner: Point { x: 1, y: 2 },
    };
    let a = w.x;
    let b = w.sum();
    w.y = 3;
    let c = takes_point(&w);
    let p: &Point = &w;
    let d = (*w).x + p.y + a + b + c;
    let mut bytes = Wrapper { inner: [1u8, 2, 3] };
    bytes[0] = 4;
    let e = takes_slice(&mut *bytes) + bytes.len();
    let ww = Wrapper { inner: Wrapper { inner: 1u8 } };
    let f: &u8 = &ww;
    let g = **ww + *f;
    let mut n = 0;
    let guard = Guard { value: &mut n };
    let h = *guard;
    let i = total(&Arr { data: [1, 2] });
    let _ = (d, e, g, h, i);
}
