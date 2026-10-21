// Traits: method resolution through bounds and through traits in scope,
// the path forms that name trait items, provided methods, supertraits,
// generic traits and impls for every kind of type.

struct S {
    x: i32,
}

struct W<T>(T);

trait Shape {
    fn area(&self) -> i32;

    fn make(x: i32) -> Self;

    // Provided methods call required ones through `self` and `Self`.
    fn double(&self) -> i32 {
        self.area() * 2
    }

    fn unit() -> Self
    where
        Self: Sized,
    {
        Self::make(1)
    }
}

impl Shape for S {
    fn area(&self) -> i32 {
        self.x
    }

    fn make(x: i32) -> Self {
        S { x }
    }
}

// A provided method overridden by the impl.
trait Greet {
    fn greet(&self) -> i32 {
        1
    }
}

impl Greet for S {
    fn greet(&self) -> i32 {
        2
    }
}

impl Greet for i32 {}

fn default_and_override(s: &S) -> i32 {
    s.greet() + 5.greet()
}

// Calls through a bound, in the parameter list or in a `where` clause.
fn through_bound<T: Shape>(t: &T) -> i32 {
    t.area() + t.double()
}

fn through_where<T>(t: T) -> T
where
    T: Shape,
{
    let n = t.area();
    T::make(n)
}

// Every path form for a trait item.
fn paths<T: Shape>(t: &T, s: &S) -> i32 {
    let a: T = T::make(1);
    let b = <T as Shape>::area(&a);
    let c: T = <T>::make(2);
    let d = Shape::area(t) + Shape::area(s);
    let e = S::make(3);
    let f = <S as Shape>::area(&e);
    let g = <S>::area(s);
    let h: S = Shape::make(4);
    let i = S::double(s);
    b + c.area() + d + f + g + h.x + i
}

// Supertraits: a bound on the subtrait gives the supertrait's methods.
trait Named: Shape {
    fn name(&self) -> i32;

    fn both(&self) -> i32 {
        self.name() + self.area()
    }
}

trait Tagged
where
    Self: Named,
{
    fn tag(&self) -> i32 {
        self.name() + self.double()
    }
}

impl Named for S {
    fn name(&self) -> i32 {
        10
    }
}

impl Tagged for S {}

fn supertraits<T: Tagged>(t: &T) -> i32 {
    t.tag() + t.both() + t.area() + T::make(1).name()
}

// Generic traits, implemented more than once for one type.
trait Into2<T> {
    fn into2(self) -> T;
}

impl Into2<i64> for i32 {
    fn into2(self) -> i64 {
        self as i64
    }
}

impl Into2<bool> for i32 {
    fn into2(self) -> bool {
        self != 0
    }
}

fn generic_trait() -> i64 {
    let a: i64 = 3.into2();
    let b: bool = 0.into2();
    let c = <i32 as Into2<i64>>::into2(4);
    if b { a } else { a + c }
}

fn generic_trait_bound<T: Into2<i64>>(t: T) -> i64 {
    t.into2()
}

// Generic methods in traits.
trait Pick {
    fn pick<U>(&self, a: U, b: U) -> U;
}

impl Pick for bool {
    fn pick<U>(&self, a: U, b: U) -> U {
        if *self { a } else { b }
    }
}

fn generic_method() -> u8 {
    let x = true.pick(1u8, 2);
    let y = false.pick::<u8>(3, 4);
    x + y + <bool as Pick>::pick::<u8>(&true, 5, 6)
}

// Impls for a generic struct, a tuple, a reference and a blanket impl.
impl<T: Shape> Shape for W<T> {
    fn area(&self) -> i32 {
        self.0.area() + 1
    }

    fn make(x: i32) -> Self {
        W(T::make(x))
    }
}

trait Sum {
    fn sum(self) -> i32;
}

impl Sum for (i32, i32) {
    fn sum(self) -> i32 {
        self.0 + self.1
    }
}

impl Sum for &S {
    fn sum(self) -> i32 {
        self.x
    }
}

trait Quad {
    fn quad(&self) -> i32;
}

impl<T: Shape> Quad for T {
    fn quad(&self) -> i32 {
        self.double() * 2
    }
}

fn impls(s: &S) -> i32 {
    let w: W<W<S>> = W::<W<S>>::make(1);
    w.area() + (1, 2).sum() + Sum::sum(s) + s.quad() + w.quad()
}

// An inherent method is found before a trait method of the same name,
// even when it takes `self` and the trait's takes `&self`; a path through
// the trait still reaches the trait's.
struct Both;

impl Both {
    fn get(self) -> u8 {
        1
    }
}

trait Get {
    fn get(&self) -> i32;
}

impl Get for Both {
    fn get(&self) -> i32 {
        2
    }
}

fn inherent_first() -> i32 {
    let a: u8 = Both.get();
    let b: i32 = Get::get(&Both);
    let c: i32 = <Both as Get>::get(&Both);
    a as i32 + b + c
}

// A trait in scope through `use`.
mod m {
    pub trait Hidden {
        fn hidden(&self) -> i32 {
            7
        }
    }

    impl Hidden for super::S {}
}

use m::Hidden;

fn in_scope(s: &S) -> i32 {
    s.hidden() + S::hidden(s)
}

// One impl for an integer type decides the literal's type.
trait Only64 {
    fn only(self) -> i64;
}

impl Only64 for i64 {
    fn only(self) -> i64 {
        self
    }
}

fn literal_receiver() -> i64 {
    1.only()
}

// `Sized` is in the prelude and every type has it.
fn sized<T: Sized>(t: T) -> T {
    t
}

// Auto-ref and auto-deref on the receiver.
trait Bump {
    fn bump(&mut self);
}

impl Bump for S {
    fn bump(&mut self) {
        self.x += 1;
    }
}

fn receivers(mut s: S, r: &mut S) -> i32 {
    s.bump();
    r.bump();
    (&mut s).bump();
    s.x + r.x
}

// Paths through `Self` and through a subtrait's bound.
trait Tr {
    fn m(&self) -> i32;

    fn f() -> i32 {
        3
    }

    fn via_self(&self) -> i32 {
        Self::m(self) + Self::f() + <Self as Tr>::m(self)
    }
}

impl Tr for S {
    fn m(&self) -> i32 {
        self.x
    }
}

trait Sub: Tr {
    fn s(&self) -> i32 {
        self.m()
    }
}

impl Sub for S {}

fn super_paths<T: Sub>(t: &T) -> i32 {
    T::f() + <T as Tr>::m(t) + T::m(t) + Tr::m(t) + t.via_self()
}

// An impl method may have fewer bounds than its trait's.
trait GB {
    fn g<U: Tr>(&self, u: U) -> i32;
}

impl GB for S {
    fn g<U>(&self, _u: U) -> i32 {
        0
    }
}

fn bounded_method<U: Tr>(s: &S, u: U) -> i32 {
    s.g(u)
}

// A generic trait implemented for a generic struct.
trait Conv<T> {
    fn conv(self) -> T;
}

impl<T> Conv<T> for W<T> {
    fn conv(self) -> T {
        self.0
    }
}

fn generic_conv() -> u8 {
    W(3u8).conv()
}

// A generic function calling itself through its bound.
fn recurse<T: Tr>(t: &T, n: i32) -> i32 {
    if n == 0 { t.m() } else { recurse(t, n - 1) }
}

// A trait method named like a free fn and an inherent method.
fn m() -> i32 {
    0
}

impl<T> W<T> {
    fn m(&self) -> u8 {
        1
    }
}

impl<T: Tr> Tr for W<T> {
    fn m(&self) -> i32 {
        self.0.m()
    }
}

fn names(w: &W<S>) -> i32 {
    let a: u8 = w.m();
    a as i32 + m() + Tr::m(w)
}

// Methods through a mutable reference.
fn mutable(s: &mut S) -> i32 {
    s.m() + s.s()
}

fn main() {
    let s = S { x: 1 };
    let _ = default_and_override(&s) + through_bound(&s) + through_where(S::unit()).x;
    let _ = paths(&s, &s) + supertraits(&s);
    let _ = generic_trait() + generic_trait_bound(1) + literal_receiver();
    let _ = generic_method() as i32 + impls(&s) + inherent_first() + in_scope(&s);
    let _ = receivers(sized(S { x: 2 }), &mut S { x: 3 });
    let _ = super_paths(&s) + bounded_method(&s, S { x: 2 }) + generic_conv() as i32;
    let _ = recurse(&s, 3) + names(&W(S { x: 3 }));
    let _ = mutable(&mut S { x: 4 });
}
