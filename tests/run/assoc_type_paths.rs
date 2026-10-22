//! Qualified paths to associated types (`<T as Tr>::Out`) whose traits
//! hold: through bounds, supertraits, `where` clauses of `impl` blocks,
//! generic `impl` blocks, and concrete types, in signatures, bounds,
//! annotations and constants.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

trait Tr {
    type Out: Copy;
    fn make(&self) -> Self::Out;
}

trait Sub: Tr {}

struct Wrap<T>(T);

impl<T: Copy> Tr for Wrap<T> {
    type Out = T;
    fn make(&self) -> T {
        self.0
    }
}

impl Tr for u16 {
    type Out = i64;
    fn make(&self) -> i64 {
        *self as i64 * 3
    }
}

impl Sub for u16 {}

struct Holder<T>(T);

impl<T> Holder<T>
where
    T: Tr,
{
    fn get(&self) -> <T as Tr>::Out {
        self.0.make()
    }
}

const SEVEN: <u16 as Tr>::Out = 7;

fn twice<T: Sub>(x: <T as Tr>::Out) -> (<T as Tr>::Out, <T as Tr>::Out) {
    (x, x)
}

fn through_impl<T: Copy>(w: Wrap<T>) -> <Wrap<T> as Tr>::Out {
    let made: <Wrap<T> as Tr>::Out = w.make();
    made
}

fn main() {
    let (a, b) = twice::<u16>(5);
    show(a + b);
    show(through_impl(Wrap(11i64)));
    show(Holder(4u16).get());
    show(Holder(Wrap(9i64)).get());
    let n: <u16 as Tr>::Out = SEVEN * 2;
    show(n);
}
