// Associated types: declared in traits, with and without bounds, defined
// by impls and used through every path form (`Self::Out`, `T::Out`,
// `<T as Tr>::Out`, `<T>::Out`, `<S as Tr>::Out`); projections that
// normalize through impls and through bindings in bounds, and stay rigid
// for parameters without bindings; defaults for trait type parameters.

struct S;

struct P(i32, i32);

struct W<T>(T);

trait Tr {
    type Out;

    fn get(&self) -> Self::Out;
}

impl Tr for S {
    type Out = i32;

    fn get(&self) -> i32 {
        1
    }
}

// The impl may also name its own type through `Self::Out`.
impl Tr for P {
    type Out = bool;

    fn get(&self) -> Self::Out {
        self.0 < self.1
    }
}

impl Tr for i32 {
    type Out = u8;

    fn get(&self) -> u8 {
        2
    }
}

// An impl for a generic type defines its type through the parameter's.
impl<T: Tr> Tr for W<T> {
    type Out = T::Out;

    fn get(&self) -> T::Out {
        self.0.get()
    }
}

// A projection on a concrete type normalizes to the impl's type.
fn concrete(s: &S, p: &P) -> i32 {
    let a: <S as Tr>::Out = s.get();
    let b: bool = p.get();
    let c: <i32 as Tr>::Out = 5.get();
    let d: i32 = W(S).get();
    let e: <W<W<P>> as Tr>::Out = W(W(P(1, 2))).get();
    if b && e { a + d + c as i32 } else { 0 }
}

// Every path form through a bound names the same rigid type.
fn forms<T: Tr>(t: &T) -> T::Out {
    let a: T::Out = t.get();
    let b: <T as Tr>::Out = a;
    let c: <T>::Out = b;
    let d: <T as Tr>::Out = c;
    d
}

fn forms_where<T>(t: &T) -> <T as Tr>::Out
where
    T: Tr,
{
    t.get()
}

// Generic code returns and passes projections.
fn pass<T: Tr>(t: &T) -> T::Out {
    forms(t)
}

fn wrapped<T: Tr>(w: &W<T>) -> T::Out {
    w.get()
}

fn instantiate(s: &S) -> i32 {
    let a: i32 = forms(s);
    let b: u8 = pass(&3);
    let c: bool = wrapped(&W(P(0, 1)));
    if c { a + b as i32 } else { 0 }
}

// A binding in a bound normalizes the projection inside the body.
fn bound_binding<T: Tr<Out = i32>>(t: &T) -> i32 {
    t.get() + 1
}

fn where_binding<T>(t: &T) -> i32
where
    T: Tr<Out = i32>,
{
    let x: T::Out = t.get();
    x * 2
}

fn call_binding(s: &S) -> i32 {
    bound_binding(s) + where_binding(&W(S)) + bound_binding::<W<W<S>>>(&W(W(S)))
}

// Bounds on associated types: the impl's type must satisfy them, and
// generic code can use them on the projection.
trait Shape {
    fn area(&self) -> i32;
}

impl Shape for S {
    fn area(&self) -> i32 {
        4
    }
}

trait Holder {
    type Item: Shape;

    fn item(&self) -> Self::Item;

    fn item_area(&self) -> i32 {
        self.item().area()
    }
}

struct Box1;

impl Holder for Box1 {
    type Item = S;

    fn item(&self) -> S {
        S
    }
}

fn holder_area<H: Holder>(h: &H) -> i32 {
    h.item().area() + h.item_area()
}

// Provided methods use `Self::Out`.
trait Counter {
    type Count;

    fn count(&self) -> Self::Count;

    fn twice(&self) -> (Self::Count, Self::Count) {
        (self.count(), self.count())
    }
}

impl Counter for S {
    type Count = u64;

    fn count(&self) -> u64 {
        3
    }
}

fn counts(s: &S) -> u64 {
    let (a, b) = s.twice();
    a + b
}

// Two traits with associated types; the same name in different traits.
trait Other {
    type Out;

    fn other(&self) -> Self::Out;
}

impl Other for S {
    type Out = bool;

    fn other(&self) -> bool {
        true
    }
}

fn two_traits<T: Tr + Other>(t: &T) -> (<T as Tr>::Out, <T as Other>::Out) {
    (Tr::get(t), Other::other(t))
}

fn two_traits_concrete(s: &S) -> i32 {
    let (a, b) = two_traits(s);
    if b { a } else { 0 }
}

// Supertraits: the subtrait's methods see the supertrait's types.
trait Sub: Tr {
    fn sub(&self) -> Self::Out {
        self.get()
    }
}

impl Sub for S {}

fn through_super<T: Sub>(t: &T) -> T::Out {
    t.sub()
}

// Defaults for trait type parameters.
trait Convert<T = i32> {
    fn convert(&self) -> T;
}

impl Convert for S {
    fn convert(&self) -> i32 {
        7
    }
}

impl Convert<bool> for S {
    fn convert(&self) -> bool {
        false
    }
}

fn convert_default<T: Convert>(t: &T) -> i32 {
    t.convert()
}

fn convert_explicit<T: Convert<bool>>(t: &T) -> bool {
    t.convert()
}

fn convert_concrete(s: &S) -> i32 {
    let a: i32 = s.convert();
    let b: bool = Convert::convert(s);
    let c = <S as Convert>::convert(s);
    let d = <S as Convert<bool>>::convert(s);
    if b || d { 0 } else { a + c + convert_default(s) }
}

// A default that names `Self`.
trait Combine<Rhs = Self> {
    type Output;

    fn combine(self, rhs: Rhs) -> Self::Output;
}

impl Combine for i32 {
    type Output = i64;

    fn combine(self, rhs: i32) -> i64 {
        (self + rhs) as i64
    }
}

impl Combine<bool> for i32 {
    type Output = i32;

    fn combine(self, rhs: bool) -> i32 {
        if rhs { self } else { 0 }
    }
}

fn combine_self<T: Combine>(a: T, b: T) -> T::Output {
    a.combine(b)
}

fn combine_bound<T: Combine<Output = i64>>(a: T, b: T) -> i64 {
    a.combine(b)
}

fn combine_concrete() -> i64 {
    let a: i64 = combine_self(1, 2);
    let b: i32 = Combine::<bool>::combine(3, true);
    a + combine_bound(4, 5) + b as i64
}

// `T::Out` in a `where` clause, through the bound on `T` beside it.
fn where_shorthand<T: Tr>(a: &T, b: &T) -> T::Out
where
    T::Out: core::ops::Add<Output = T::Out>,
{
    a.get() + b.get()
}

// A bound on a projection, checked at the call with the projection
// normalized.
trait Marker {}

impl Marker for i32 {}

fn marked<T: Tr>(_x: &T)
where
    <T as Tr>::Out: Marker,
{
}

// Bounds on associated types: `PartialEq` with its default parameter,
// `Self::N`, and `Add` with a binding the generic code relies on.
trait Num {
    type N: PartialEq + core::ops::Add<i32, Output = i32>;
    fn n(&self) -> Self::N;
}

struct Nv(i32);

impl Num for Nv {
    type N = i32;
    fn n(&self) -> i32 {
        self.0
    }
}

fn eq_ns<T: Num>(a: &T, b: &T) -> bool {
    a.n() == b.n()
}

fn add_n<T: Num>(a: &T) -> i32 {
    a.n() + 1
}

// An associated type bounded by its own trait.
trait Nested {
    type Inner: Nested;
    fn inner(&self) -> Self::Inner;
}

impl Nested for S {
    type Inner = S;
    fn inner(&self) -> S {
        S
    }
}

fn nested<T: Nested>(x: &T) -> <T::Inner as Nested>::Inner {
    x.inner().inner()
}

fn main() {
    let s = S;
    let _ = concrete(&s, &P(1, 2)) + instantiate(&s) + call_binding(&s);
    let _ = holder_area(&Box1) + two_traits_concrete(&s) + through_super(&s);
    let _ = convert_concrete(&s);
    let _ = counts(&s);
    let _ = combine_concrete();
    let _ = convert_explicit(&s);
    let _ = forms_where(&P(1, 2));
    let _ = where_shorthand(&s, &s);
    marked(&s);
    let _ = eq_ns(&Nv(1), &Nv(2)) && add_n(&Nv(3)) == 4;
    let _ = nested(&s);
}
