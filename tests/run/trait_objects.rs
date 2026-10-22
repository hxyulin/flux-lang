//! Trait objects: heterogeneous arrays of `&dyn Shape` dispatched
//! through vtables, with provided and supertrait methods, `&mut dyn`
//! mutation, upcasting to a supertrait's object, generic traits
//! (`dyn Conv<i32>`), associated types (`dyn Source<Out = i64>`), raw
//! pointers to trait objects, and generic code over `T: ?Sized`.

unsafe extern "C" { fn printf(format: *const i8, ...) -> i32; }

trait Named {
    fn id(&self) -> i32;
    fn describe(&self) -> i32 { self.id() * 100 }
}

trait Shape: Named {
    fn area(&self) -> i32;
    fn scaled(&self, k: i32) -> i32 { self.area() * k }
    fn grow(&mut self, by: i32);
}

struct Square { side: i32 }
struct Rect { w: i32, h: i32 }

impl Named for Square { fn id(&self) -> i32 { 1 } }
impl Named for Rect { fn id(&self) -> i32 { 2 } fn describe(&self) -> i32 { -2 } }

impl Shape for Square {
    fn area(&self) -> i32 { self.side * self.side }
    fn grow(&mut self, by: i32) { self.side += by; }
}
impl Shape for Rect {
    fn area(&self) -> i32 { self.w * self.h }
    fn scaled(&self, k: i32) -> i32 { self.w * self.h * k + 1 }
    fn grow(&mut self, by: i32) { self.w += by; self.h += by; }
}

fn total(shapes: &[&dyn Shape]) -> i32 {
    let mut t = 0;
    for s in shapes {
        t += s.area() + s.scaled(2) + s.describe() + s.id();
    }
    t
}

fn grow_all(shapes: &mut [&mut dyn Shape], by: i32) {
    for s in shapes.iter_mut() {
        s.grow(by);
    }
}

fn as_named(s: &dyn Shape) -> &dyn Named { s }

fn generic<T: Shape + ?Sized>(x: &T) -> i32 { x.area() + x.id() }

trait Conv<T> { fn conv(&self, x: T) -> T; }
struct Add(i32);
impl Conv<i32> for Add { fn conv(&self, x: i32) -> i32 { x + self.0 } }
impl Conv<u8> for Add { fn conv(&self, x: u8) -> u8 { x.wrapping_add(self.0 as u8) } }

trait Source { type Out; fn get(&self) -> Self::Out; }
struct Seven;
impl Source for Seven { type Out = i64; fn get(&self) -> i64 { 7 } }

fn read(s: &dyn Source<Out = i64>) -> i64 { s.get() * 2 }

fn main() {
    let sq = Square { side: 3 };
    let re = Rect { w: 2, h: 5 };
    let shapes: [&dyn Shape; 2] = [&sq, &re];
    let mut a = Square { side: 1 };
    let mut b = Rect { w: 1, h: 2 };
    {
        let mut ms: [&mut dyn Shape; 2] = [&mut a, &mut b];
        grow_all(&mut ms, 2);
        ms[0].grow(1);
    }
    let n: &dyn Named = as_named(&re);
    let up: &dyn Named = shapes[0];
    let c: &dyn Conv<i32> = &Add(10);
    let d: &dyn Conv<u8> = &Add(250);
    let p: *const dyn Shape = &sq;
    unsafe {
        printf(c"%d %d %d\n".as_ptr(), total(&shapes), a.side, b.w * b.h);
        printf(c"%d %d %d %d\n".as_ptr(), n.id(), n.describe(), up.describe(), generic(shapes[1]));
        printf(c"%d %d %lld\n".as_ptr(), c.conv(5), d.conv(10) as i32, read(&Seven));
        printf(c"%d\n".as_ptr(), (*p).area());
    }
}
