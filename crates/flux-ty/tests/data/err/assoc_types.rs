// Errors involving associated types: paths that name no type or an
// ambiguous one, impls whose items do not match the trait, bounds on
// associated types and bindings that do not hold, and projections that
// stay rigid in generic code, so they differ from every other type.

struct S;

struct NotShape;

trait Shape {
    fn area(&self) -> i32;
}

impl Shape for S {
    fn area(&self) -> i32 {
        1
    }
}

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

trait Other {
    type Out;
}

trait NoTypes {
    fn m(&self);
}

// E0220: the type is not found through the bound, or there is no bound.
fn unbounded<T>(_x: T::Out) {}

fn not_in_bound<T: Tr>(_x: T::Nope) {}

trait SelfNope {
    fn get(&self) -> Self::Nope;
}

fn unknown_binding<T: Tr<Nope = i32>>(_x: T) {}

fn bound_without_types<T: NoTypes>(_x: T::Out) {}

// E0221: two bounds both declare the type.
fn ambiguous_bounds<T: Tr + Other>(_x: T::Out) {}

// E0223: a type path on a concrete type needs the trait.
fn concrete_shorthand() {
    let _x: S::Out = 1;
}

impl S {
    fn inherent_self(&self) -> Self::Out {
        1
    }
}

// E0576: a qualified path names a type the trait does not have.
fn qualified_unknown() {
    let _x: <S as Tr>::Nope = 1;
}

// E0277: a qualified path whose type does not implement the trait.
fn qualified_not_impl() {
    let _x: <NotShape as Tr>::Out = 1;
}

// E0437, E0325, E0046: impl items that do not match the trait.
struct A;
struct B;
struct C;

impl Shape for A {
    type Out = i32;

    fn area(&self) -> i32 {
        1
    }
}

impl NoTypes for B {
    type m = i32;
}

impl Tr for C {
    fn get(&self) -> i32 {
        1
    }
}

// E0324: a method where the trait has a type.
struct E;

impl Other for E {
    fn Out() {}
}

// E0277: the impl's type does not satisfy the bound on the trait's.
trait Holder {
    type Item: Shape;
}

impl Holder for S {
    type Item = NotShape;
}

impl Holder for A {
    type Item = S;
}

struct W<T>(T);

impl<T> Holder for W<T> {
    type Item = T;
}

impl<T: Shape> Other for W<T> {
    type Out = T;
}

// E0053: the impl method's signature uses another type than the impl's.
struct D;

impl Tr for D {
    type Out = i32;

    fn get(&self) -> u8 {
        1
    }
}

// E0271: a binding in a bound does not hold for the argument.
fn wants_u8<T: Tr<Out = u8>>(_x: T) {}

fn wants_u8_turbofish<T: Tr<Out = u8>>() {}

fn binding_mismatch() {
    wants_u8(S);
    wants_u8_turbofish::<S>();
}

fn binding_mismatch_generic<U: Tr<Out = i32>>(u: U) {
    wants_u8(u);
}

// E0308: a projection on a parameter is rigid.
fn rigid_vs_i32<T: Tr>(x: T) -> i32 {
    x.get()
}

fn rigid_vs_param<T: Tr>(x: T) -> T {
    x.get()
}

fn param_vs_rigid<T: Tr>(x: T) -> T::Out {
    x
}

fn two_params<T: Tr, U: Tr>(_a: T, b: U) -> T::Out {
    b.get()
}

fn unbounded_qself<T>(_x: <T>::Out) {}

fn two_traits<T: Tr + Other>(x: &T) -> <T as Other>::Out {
    x.get()
}

// A binding fixes it, and a concrete type normalizes it.
fn binding_vs_u8<T: Tr<Out = i32>>(x: T) -> u8 {
    x.get()
}

fn binding_vs_bool<T: Tr<Out = i32>>() -> T::Out {
    true
}

fn normalized_vs_u8() {
    let _x: u8 = S.get();
    let _y: <S as Tr>::Out = true;
}

// A defaulted trait parameter: an impl that leaves it out implements the
// default, and a bound takes no more arguments than the trait declares.
trait Convert<T = i32> {
    fn convert(&self) -> T;
}

impl Convert for S {
    fn convert(&self) -> bool {
        true
    }
}

fn too_many<T: Convert<i32, i32>>(_x: T) {}

// E0271: a rigid projection where a bound binds it.
trait Rigid {
    type Out;
}

fn wants_u8_out<T: Rigid<Out = u8>>(_x: T) {}

fn rigid_binding<U: Rigid>(u: U) {
    wants_u8_out(u);
}

// A bound on an associated type with a binding: the impl's type must
// implement the trait (rustc words `Add` bounds as "cannot add"), and its
// `Output` must be the bound's.
trait AddsI32 {
    type N: core::ops::Add<i32, Output = i32>;
}

struct Bytes;

impl AddsI32 for Bytes {
    type N = u8;
}

struct Odd;

impl core::ops::Add<i32> for Odd {
    type Output = u8;
    fn add(self, _rhs: i32) -> u8 {
        0
    }
}

struct Odds;

impl AddsI32 for Odds {
    type N = Odd;
}

// E0229: bindings belong in trait bounds only.
fn binding_on_struct(_x: W<Out = i32>) {}

struct Header;

impl Rigid<Out = i32> for Header {
    type Out = i32;
}

fn main() {}
