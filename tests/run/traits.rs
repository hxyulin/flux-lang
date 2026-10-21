//! Traits: required and provided methods, with and without `self`,
//! overridden defaults, supertraits, generic traits, generic methods in
//! traits, impls for structs, generic structs, primitives, tuples and
//! references, blanket impls, bounds in parameter lists and `where`
//! clauses, method calls through bounds and through traits in scope,
//! `T::f()`, `Self::f()`, `<T as Tr>::f()`, `Tr::f(x)`, two traits with
//! one method name told apart by path, and generic code calling generic
//! code through several levels of bounds.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

trait Shape {
    fn area(&self) -> i32;
    fn scaled(&self, k: i32) -> i32 {
        self.area() * k
    }
    fn unit() -> Self;
    fn twice_unit_area() -> i32
    where
        Self: Sized,
    {
        Self::unit().area() * 2
    }
}

trait Named: Shape {
    fn id(&self) -> i32;
    fn tag(&self) -> i32 {
        self.id() * 1000 + self.area()
    }
}

struct Sq(i32);

struct Rect {
    w: i32,
    h: i32,
}

impl Shape for Sq {
    fn area(&self) -> i32 {
        self.0 * self.0
    }
    fn unit() -> Self {
        Sq(1)
    }
}

impl Shape for Rect {
    fn area(&self) -> i32 {
        self.w * self.h
    }
    fn scaled(&self, k: i32) -> i32 {
        self.area() * k + 1
    }
    fn unit() -> Rect {
        Rect { w: 1, h: 2 }
    }
}

impl Named for Rect {
    fn id(&self) -> i32 {
        7
    }
}

struct W<T> {
    inner: T,
}

impl<T: Shape> Shape for W<T> {
    fn area(&self) -> i32 {
        self.inner.area() + 100
    }
    fn unit() -> Self {
        W { inner: T::unit() }
    }
}

trait Double {
    fn double(self) -> Self;
}

impl Double for i32 {
    fn double(self) -> i32 {
        self * 2
    }
}

impl Double for u8 {
    fn double(self) -> u8 {
        self.wrapping_mul(2)
    }
}

impl Double for (u8, u8) {
    fn double(self) -> (u8, u8) {
        (self.0.double(), self.1.double())
    }
}

// A generic trait, implemented twice for one type.
trait Scale<T> {
    fn scale(&self, by: T) -> i64;
}

impl Scale<i32> for Sq {
    fn scale(&self, by: i32) -> i64 {
        (self.area() * by) as i64
    }
}

impl Scale<u8> for Sq {
    fn scale(&self, by: u8) -> i64 {
        (self.area() as i64) << by
    }
}

// A trait with a generic method.
trait Visit {
    fn visit<V: Shape>(&self, v: &V) -> i32;
}

impl Visit for Rect {
    fn visit<V: Shape>(&self, v: &V) -> i32 {
        self.area() * 10 + v.area()
    }
}

// Through references, and a blanket impl over every `Named` type.
trait Describe {
    fn describe(self) -> i32;
}

impl Describe for &Sq {
    fn describe(self) -> i32 {
        -self.area()
    }
}

trait Label {
    fn label(&self) -> i32;
}

impl<T: Named> Label for T {
    fn label(&self) -> i32 {
        self.tag() + 1
    }
}

// Two traits with a method of one name.
trait Left {
    fn side(&self) -> i32;
}

trait Right {
    fn side(&self) -> i32;
}

impl Left for Sq {
    fn side(&self) -> i32 {
        1
    }
}

impl Right for Sq {
    fn side(&self) -> i32 {
        2
    }
}

// An inherent method of the same name as a trait's wins.
impl Sq {
    fn area(&self) -> i32 {
        -1
    }
}

fn total<T: Shape>(a: &T, b: &T) -> i32 {
    a.area() + T::scaled(b, 3) + <T as Shape>::area(a) + Shape::area(b)
}

fn tagged<T>(x: &T) -> i32
where
    T: Named,
{
    x.tag() + x.area() + x.label()
}

fn make<T: Shape>() -> T {
    T::unit()
}

fn quad<T: Double>(x: T) -> T {
    x.double().double()
}

fn deep<T: Shape>(x: &T) -> i32 {
    let w = W { inner: make::<T>() };
    total(&w, &w) + x.scaled(2) + T::twice_unit_area()
}

fn main() {
    let s = Sq(3);
    let r = Rect { w: 2, h: 5 };
    let w = W { inner: Sq(4) };
    let ww: W<W<Rect>> = make();
    let p = quad((1u8, 200u8));
    unsafe {
        printf(
            c"%d %d %d %d\n".as_ptr(),
            s.area(),
            Shape::area(&s),
            s.scaled(2),
            Sq::unit().0,
        );
        printf(
            c"%d %d %d\n".as_ptr(),
            r.area(),
            r.scaled(2),
            Rect::unit().area(),
        );
        printf(c"%d %d\n".as_ptr(), total(&s, &s), total(&r, &r));
        printf(c"%d %d %d\n".as_ptr(), tagged(&r), r.tag(), r.label());
        printf(c"%d %d %d\n".as_ptr(), w.area(), w.scaled(2), ww.area());
        printf(
            c"%d %d %d %d\n".as_ptr(),
            quad(5),
            7.double(),
            p.0 as i32,
            p.1 as i32,
        );
        printf(
            c"%lld %lld\n".as_ptr(),
            s.scale(2i32),
            <Sq as Scale<u8>>::scale(&s, 3),
        );
        printf(c"%d %d\n".as_ptr(), r.visit(&s), r.visit(&ww));
        let rs = &s;
        printf(c"%d %d\n".as_ptr(), (&s).describe(), rs.describe());
        printf(
            c"%d %d %d\n".as_ptr(),
            Left::side(&s),
            <Sq as Right>::side(&s),
            Sq::twice_unit_area(),
        );
        printf(c"%d %d\n".as_ptr(), deep(&s), deep(&r));
        let m: Sq = make();
        printf(c"%d\n".as_ptr(), Shape::area(&m));
    }
}
