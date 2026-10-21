// Traits: definitions, impls, bounds, `where` clauses and the paths that
// name trait items.

struct S {
    x: i32,
}

struct W<T>(T);

// Required and provided methods, with and without `self`.
trait Shape {
    fn area(&self) -> i32;

    fn new() -> Self;

    fn double(&self) -> i32 {
        self.area() * 2
    }

    fn zero() -> i32 {
        0
    }
}

// A bodiless method may take `self` by value; a provided one needs
// `Self: Sized`.
trait Consume {
    fn consume(self) -> i32;

    fn consume_twice(self) -> i32
    where
        Self: Sized,
    {
        self.consume()
    }
}

// Generic methods and generic traits.
trait Convert<T> {
    fn convert(&self) -> T;

    fn with<U>(&self, u: U) -> U {
        u
    }
}

// Supertraits, in the header and in a `where` clause.
trait Named: Shape {
    fn name(&self) -> i32;
}

trait Both
where
    Self: Named + Consume,
{
}

// Parameters may be patterns when the method has a body.
trait Patterns {
    fn sum(&self, (a, b): (i32, i32)) -> i32 {
        a + b
    }
}

impl Shape for S {
    fn area(&self) -> i32 {
        self.x
    }

    fn new() -> Self {
        S { x: Self::zero() }
    }
}

impl Consume for S {
    fn consume(self) -> i32 {
        self.x
    }
}

impl Named for S {
    fn name(&self) -> i32 {
        1
    }
}

impl Both for S {}

impl Convert<bool> for S {
    fn convert(&self) -> bool {
        self.x > 0
    }
}

impl Patterns for S {}

// Impls for a generic struct, a primitive, a tuple and a reference.
impl<T: Shape> Shape for W<T> {
    fn area(&self) -> i32 {
        self.0.area()
    }

    fn new() -> Self {
        W(T::new())
    }
}

impl Consume for i32 {
    fn consume(self) -> i32 {
        self
    }
}

impl Consume for (i32, i32) {
    fn consume(self) -> i32 {
        self.0 + self.1
    }
}

impl Consume for &S {
    fn consume(self) -> i32 {
        self.x
    }
}

// A blanket impl.
trait Area2 {
    fn area2(&self) -> i32;
}

impl<T: Shape> Area2 for T {
    fn area2(&self) -> i32 {
        self.area() + self.area()
    }
}

// Bounds in the parameter list and in `where` clauses.
fn bound<T: Shape>(x: &T) -> i32 {
    x.area()
}

fn bounds<T: Shape + Consume, U: Convert<bool>>(x: T, u: &U) -> i32 {
    let _b: bool = u.convert();
    x.consume()
}

fn where_clause<T>(x: T) -> i32
where
    T: Consume + Sized,
{
    x.consume()
}

fn where_on_concrete() -> i32
where
    S: Shape,
{
    0
}

impl<T> W<T>
where
    T: Shape,
{
    fn inner_area(&self) -> i32 {
        self.0.area()
    }
}

// Paths to trait items: through a type parameter, `Self`, a type, a
// qualified path and the trait itself.
fn paths<T: Shape>(s: &S, t: &T) -> i32 {
    let _a: T = T::new();
    let _b: S = S::new();
    let _c = <T as Shape>::area(t);
    let _d = <S as Shape>::new();
    let _e = <T>::zero();
    let _f = Shape::area(s);
    let _g = <S as Convert<bool>>::convert(s);
    let _h = Shape::double(t);
    S::zero()
}

// The trait of a method call must be in scope; `use` brings it there.
mod m {
    pub trait Hidden {
        fn hidden(&self) -> i32 {
            7
        }
    }

    impl Hidden for super::S {}

    pub mod inner {
        pub trait Deep {
            fn deep(&self) -> i32;
        }
    }
}

use m::Hidden;
use m::inner::Deep;

impl m::inner::Deep for S {
    fn deep(&self) -> i32 {
        3
    }
}

fn in_scope(s: &S) -> i32 {
    s.hidden() + s.deep() + m::inner::Deep::deep(s)
}

fn generic_method(s: &S) -> u8 {
    s.with::<u8>(1)
}

fn main() {
    let s = S { x: 1 };
    let _ = bound(&s) + paths(&s, &s) + in_scope(&s) + where_on_concrete();
    let _ = bounds(S { x: 2 }, &s) + where_clause(3) + generic_method(&s) as i32;
    let _ = W(S { x: 4 }).inner_area() + (1, 2).consume() + Consume::consume(&s);
    let _ = s.area2() + s.sum((1, 2)) + s.name() + s.consume_twice();
}
