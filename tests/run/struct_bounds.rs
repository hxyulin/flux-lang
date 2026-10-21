//! Bounds on structs: a struct's bounds let its fields name associated
//! types (`T::Out`) and its `impl` blocks call the bounds' methods;
//! derived impls and `Drop` impls carry them.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn say(n: i32) {
    unsafe {
        printf(c"%d\n".as_ptr(), n);
    }
}

trait Shape {
    type Out;
    fn area(&self) -> i32;
    fn make(&self) -> Self::Out;
}

#[derive(Clone, Copy, PartialEq)]
struct Square(i32);

impl Shape for Square {
    type Out = i32;
    fn area(&self) -> i32 {
        self.0 * self.0
    }
    fn make(&self) -> i32 {
        self.0 + 1
    }
}

struct Pair(i32, i32);

impl Shape for Pair {
    type Out = (i32, i32);
    fn area(&self) -> i32 {
        self.0 * self.1
    }
    fn make(&self) -> (i32, i32) {
        (self.1, self.0)
    }
}

// The field's type is the bound's associated type.
struct Cached<S: Shape> {
    shape: S,
    made: S::Out,
}

impl<S: Shape> Cached<S> {
    fn new(shape: S) -> Self {
        let made = shape.make();
        Cached { shape, made }
    }

    fn area(&self) -> i32 {
        self.shape.area()
    }
}

#[derive(Clone, PartialEq)]
struct Labeled<T>
where
    T: Shape + Clone,
{
    label: i32,
    shape: T,
}

struct Loud<T: Shape>(T);

impl<T: Shape> Drop for Loud<T> {
    fn drop(&mut self) {
        say(self.0.area());
    }
}

fn total<T: Shape>(c: &Cached<T>, extra: i32) -> i32 {
    c.area() + extra
}

// A bound saying what the field's type is.
fn doubled<T: Shape<Out = i32>>(c: &Cached<T>) -> i32 {
    c.made * 2
}

// Moving the field out drops the rest.
fn made<T: Shape>(c: Cached<Loud<T>>) -> <Loud<T> as Shape>::Out {
    c.made
}

impl<T: Shape> Shape for Loud<T> {
    type Out = T::Out;
    fn area(&self) -> i32 {
        self.0.area() + 100
    }
    fn make(&self) -> T::Out {
        self.0.make()
    }
}

fn main() {
    let c = Cached::new(Square(3));
    say(c.area());
    say(c.made);
    let p = Cached::new(Pair(2, 5));
    say(total(&p, 1));
    say(p.made.0 * 10 + p.made.1);
    let l = Labeled { label: 7, shape: Square(4) };
    let m = l.clone();
    say((l == m) as i32);
    say(m.label + m.shape.area());
    {
        let _a = Loud(Square(2));
        let _b = Loud(Pair(3, 3));
    }
    say(0);
    say(doubled(&c));
    let m = made(Cached::new(Loud(Pair(4, 6))));
    say(m.0 - m.1);
    say(0);
}
