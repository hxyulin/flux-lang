// Type errors involving traits: unsatisfied bounds, methods that cannot be
// found or are ambiguous, and impls that do not match their trait.

struct S;

struct NotTr;

struct W<T>(T);

trait Tr {
    fn m(&self) -> i32;

    fn f() -> i32 {
        0
    }
}

impl Tr for S {
    fn m(&self) -> i32 {
        1
    }
}

fn take<T: Tr>(_x: T) {}

fn take_ref<T: Tr>(_x: &T) {}

fn second<T: Tr>(_a: i32, _b: T) {}

fn twice<T: Tr>(_a: T, _b: T) {}

fn make<T: Tr>() -> T {
    loop {}
}

fn turbofish<T: Tr>(_x: T) {}

fn wrapped<T: Tr>(_w: W<T>) {}

fn where_bound<T>(_x: T)
where
    T: Tr,
{
}

// E0277 points at the argument whose type holds the parameter, at the
// callee when two do, at the call when only the return type does, and at
// the turbofish argument when there is one.
fn bound_at_argument() {
    take(NotTr);
    take_ref(&NotTr);
    second(1, NotTr);
    wrapped(W(NotTr));
    where_bound(NotTr);
}

fn bound_at_callee() {
    twice(NotTr, NotTr);
}

fn bound_at_call() {
    let _x: NotTr = make();
}

fn bound_at_turbofish() {
    turbofish::<NotTr>(NotTr);
    let _y = make::<NotTr>();
}

fn bound_on_primitive() {
    take(1u8);
    take(true);
}

// References and tuples need impls of their own.
fn bound_on_compound() {
    take(&S);
    take((S, S));
}

// Two unsatisfied bounds are two errors at the same place.
trait Other {}

fn two_bounds<T: Tr + Other>(_x: T) {}

fn bound_twice() {
    two_bounds(NotTr);
}

// A qualified path checks its self type.
fn bound_at_qself() {
    let _ = <NotTr as Tr>::f();
    let _ = <NotTr as Tr>::m(&NotTr);
}

// A path through the trait checks the argument.
fn bound_at_trait_path() {
    let _ = Tr::m(&NotTr);
}

// A bound in generic code that the caller's parameter does not have.
fn generic_caller<T>(x: T) {
    take(x);
}

fn generic_qself<T>(x: &T) -> i32 {
    <T as Tr>::m(x)
}

// Methods that do not exist, or whose trait is not implemented.
fn no_method(n: NotTr) {
    n.m();
}

fn no_assoc_fn() {
    let _ = NotTr::f();
}

fn unbounded<T>(x: T) {
    x.m();
}

fn unbounded_path<T>(x: &T) {
    let _ = T::f();
    let _ = T::m(x);
}

fn assoc_fn_as_method(s: S) {
    s.f();
}

// A generic impl whose bounds the receiver does not meet.
impl<T: Tr> Tr for W<T> {
    fn m(&self) -> i32 {
        self.0.m()
    }
}

fn no_method_on_reference() {
    (&NotTr).m();
}

fn qualified_wrong_argument<T>(x: &T) {
    <S as Tr>::m(x);
}

fn unsatisfied_impl(w: W<NotTr>) {
    w.m();
    W(1).m();
}

trait Blanket {
    fn blanket(&self) {}

    fn blanket_fn() {}
}

impl<T: Tr> Blanket for T {}

fn unsatisfied_blanket(n: NotTr) {
    n.blanket();
    NotTr::blanket_fn();
}

// A trait that is implemented but not in scope.
mod m {
    pub trait Hidden {
        fn hidden(&self) {}

        fn hidden_fn() {}
    }

    impl Hidden for super::S {}
}

fn not_in_scope(s: S) {
    s.hidden();
    S::hidden_fn();
}

// Two traits with the same method or function, through the type, two
// bounds or a common subtrait.
trait A {
    fn same(&self) {}

    fn same_fn() {}
}

trait B {
    fn same(&self) {}

    fn same_fn() {}

    fn b(&self) {}
}

impl A for S {}

impl B for S {}

fn ambiguous(s: S) {
    s.same();
    S::same_fn();
}

fn ambiguous_bound<T: A + B>(t: T) {
    t.same();
}

trait AB: A + B {}

fn ambiguous_super<T: AB>(t: T) {
    t.same();
}

// A bound on one trait does not give another's methods.
fn other_trait<T: A>(t: T) {
    t.b();
}

// An associated function called on the trait alone.
fn no_self_type() {
    let _ = Tr::f();
}

// A supertrait the type does not implement.
trait Sub: Other {}

impl Sub for S {}

// A `where` clause that can never hold.
fn never()
where
    NotTr: Tr,
{
}

// A provided method that moves an unsized `Self`, binds it or passes it
// on.
trait Consume {
    fn consume(self) {}

    fn bind(self) {
        let _x = self;
    }

    fn pass(self) {
        self.consume()
    }
}

// A generic trait implemented for another argument.
trait Conv<T> {
    fn conv(&self) -> T;
}

impl Conv<bool> for S {
    fn conv(&self) -> bool {
        true
    }
}

// Generic traits take their arguments.
trait Gen2<T> {}

impl Gen2 for S {}

impl Gen2<i32, u8> for NotTr {}

fn missing_trait_args<T: Gen2>(_t: T) {}

fn extra_trait_args<T: Tr<i32>>(_t: T) {}

fn wants_i64<T: Conv<i64>>(_t: T) {}

fn wrong_trait_argument() {
    wants_i64(S);
}

// A trait method's result has the declared type.
fn wrong_result(s: S) {
    let _: u8 = s.m();
    let _: bool = Tr::m(&s);
}

trait Pairs {
    fn pairs(&self) {}
}

impl Pairs for (i32, i32) {}

fn tuple_instance() {
    (1u8, 2u8).pairs();
}

// Argument counts in calls of trait methods.
fn arg_counts(s: &S) {
    s.m(1);
    let _ = <S as Tr>::f(1, 2);
    let _ = Tr::m(s, s);
}

// Unknown methods inside a provided method.
trait Provided {
    fn p(&self) {
        self.nope();
    }
}

// Bounds on an inherent impl, and on a method.
impl<T: Tr> W<T> {
    fn get(&self) {}

    fn new(x: T) -> Self {
        W(x)
    }
}

impl<T> W<T> {
    fn checked(&self)
    where
        T: Tr,
    {
    }

    fn put<U: Tr>(&self, _u: U) {}
}

fn inherent_bounds(w: W<NotTr>) {
    w.get();
    let _ = W::new(NotTr);
    let _ = W::<NotTr>::new(NotTr);
    w.checked();
    w.put(NotTr);
    w.put::<NotTr>(NotTr);
}

// Generic arguments on trait methods.
trait GenM {
    fn g<T>(&self, x: T) {}
}

impl GenM for S {}

fn method_generics(s: S) {
    s.g::<i32, u8>(1);
}

// An impl for a reference.
trait ByRef {
    fn by_ref(self);
}

impl ByRef for &S {
    fn by_ref(self) {}
}

fn through_ref(s: S) {
    ByRef::by_ref(s);
}

// Impls that do not match their trait.
trait Req {
    fn one(&self) -> i32;

    fn two(&self, x: i32) -> i32;

    fn three();

    fn provided(&self) {}
}

impl Req for NotTr {
    fn one(&self) -> i32 {
        1
    }
}

impl Req for W<i32> {
    fn one(&self) -> u8 {
        1
    }

    fn two(&self, x: u8) -> i32 {
        0
    }

    fn three() {}

    fn four(&self) {}
}

impl Req for W<u8> {
    fn one(&mut self) -> i32 {
        1
    }

    fn two(&self) -> i32 {
        0
    }

    fn three(&self) {}
}

impl Req for W<bool> {
    fn one() -> i32 {
        1
    }

    fn two(&self, x: i32, y: i32) -> i32 {
        0
    }

    fn three() {}
}

impl Conv<u8> for NotTr {
    fn conv(&self) -> i8 {
        0
    }
}

trait Make {
    fn make() -> Self;
}

impl Make for NotTr {
    fn make() -> i32 {
        0
    }
}

impl Req for W<u16> {
    fn one(self) -> i32 {
        1
    }

    fn two(&self, x: i32) {}

    fn three() {}
}

// Generic method parameters must match.
trait Gen {
    fn g<T>(&self, x: T);

    fn h(&self);
}

impl Gen for S {
    fn g(&self, x: i32) {}

    fn h<T>(&self) {}
}

// Overlapping impls.
trait Over {}

impl Over for S {}

impl Over for S {}

impl<T: Tr> Over for W<T> {}

impl Over for W<S> {}

trait Q {}

impl<T: Tr> Q for T {}

impl Q for S {}

impl Q for NotTr {}

// `unsafe` on the wrong side.
trait Safe {}

unsafe trait Unsafe {}

unsafe impl Safe for S {}

impl Unsafe for S {}


fn main() {}
