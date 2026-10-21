// Traits: names and items rejected while lowering.

struct S;

trait Tr {
    fn m(&self);

    fn f() {}
}

impl Tr for S {
    fn m(&self) {}
}

// A path that must name a trait names something else, or nothing.
impl S for i32 {}

impl Nope for S {}

trait Sub: S {}

trait Sub2: Missing {}

fn bound_struct<T: S>(_x: T) {}

fn bound_unknown<T: Unknown>(_x: T) {}

fn where_struct<T>(_x: T)
where
    T: S,
{
}

fn where_unknown<T>(_x: T)
where
    T: Gone + Tr,
{
}

fn qualified_struct() {
    <S as S>::f();
}

fn qualified_unknown() {
    <S as Absent>::f();
}

// An item the trait does not have, named through the trait.
fn not_in_trait() {
    <S as Tr>::nope();
}

fn not_in_trait_method(s: &S) {
    <S as Tr>::other(s);
}

// Duplicate items in a trait, with or without bodies, and a trait named
// like a struct.
trait Dup {
    fn a(&self);

    fn a(&self);

    fn b() {}

    fn b(&self);
}

struct Clash;

trait Clash {}

// Trait items share the trait's visibility.
pub trait Vis {
    pub fn v(&self);
}

impl Vis for S {
    pub fn v(&self) {}
}

// Parameters bound twice, in a method with a body.
trait Twice {
    fn twice(a: i32, a: i32) {}

    fn once(a: i32, a: i32);
}

// Patterns need a body.
trait Pat {
    fn pat(&self, (a, b): (i32, i32));

    fn pat_ok(&self, (a, b): (i32, i32)) {}
}

// A private trait.
mod m {
    trait Private {
        fn p() {}
    }

    pub struct P;

    impl Private for P {}
}

fn private_trait() {
    m::Private::p();
}

fn main() {}
