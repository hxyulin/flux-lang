// Types that name a struct must meet its bounds (E0277), as rustc
// checks: in signatures, which do not assume a struct's bounds, so
// `fn f<T>(w: W<T>)` needs `T: Tr`; in `impl` headers and the trait
// signatures their methods implement; in annotations, paths and
// generic arguments in bodies; and in constructors and struct
// expressions, at the value whose type has the parameter if one does.
// A struct's own bounds must hold too (`where u8: Tr`), and so must
// what a bound says an associated type is, unless the trait itself does
// not hold. Each unmet bound is reported once, at the innermost type that
// needs it, as its innermost cause through `impl` blocks; types in bounds
// are checked too, and so are struct patterns.

trait Tr {}

impl Tr for i32 {}

trait Shape {
    type Out;
}

impl Shape for i32 {
    type Out = u8;
}

struct W<T: Tr>(T);

struct V<T>
where
    T: Tr,
{
    x: T,
}

struct P<T: Tr>(T, T);

struct R<T: Tr> {
    a: i32,
    b: (T, i32),
}

struct Fixed<S: Shape<Out = i32>>(S);

struct Holder {
    w: W<u8>,
}

struct Trivial
where
    u8: Tr,
{
    x: i32,
}

fn f<T>(w: W<T>) {}

fn g(x: W<u8>) -> V<bool> {
    loop {}
}

fn fixed(f: Fixed<i32>) {}

impl<T> W<T> {
    fn m(&self) {}
}

impl W<u16> {}

trait Q {
    fn q(&self, w: W<u32>);
}

impl<T: Tr> W<T> {
    fn new(t: T) -> Self {
        W(t)
    }
}

impl<T> Drop for V<T> {
    fn drop(&mut self) {}
}

fn generic<T>(t: T) {
    let w = W(t);
}

fn id<T>(t: T) -> T {
    t
}

fn nested(a: &W<u8>, b: (i32, W<u16>), c: W<W<u32>>) {}

trait Blanket {}

impl<T: Copy> Blanket for T {}

struct B<T: Blanket>(T);

struct NotCopy;

fn leaf(b: B<NotCopy>) {}

trait Holds<X> {
    type A;
}

fn in_bounds<T, X: Holds<W<u8>>, Y: Holds<i32, A = W<u16>>>()
where
    W<T>: Sized,
{
}

fn wrong_trait(f: Fixed<bool>) {}

trait Takes {
    fn take(&self, w: W<Self>)
    where
        Self: Sized + Tr;
}

struct S;

impl Takes for S {
    fn take(&self, w: u8) {}
}

fn main() {
    let W(p) = W(1u64);
    let a = W(1u8);
    let b: W<u64> = loop {};
    let c = V { x: 1u8 };
    let d = W::<i8>(1);
    let ok = W(1i32);
    let p = P(1u8, 2u8);
    let r = R { a: 1, b: (1u8, 2) };
    let s: R<u16> = R { b: (1, 2), a: 1 };
    let n = W::<u8>::new(1);
    let m = <W<u16>>::new(1);
    let i = id::<W<u32>>(loop {});
    let e: Fixed<i32> = loop {};
    let h = Fixed(1i32);
}
