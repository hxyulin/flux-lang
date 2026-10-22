// An associated type needs its trait: `<T as Tr>::Out` needs `T: Tr`
// (E0277) wherever it is written, as rustc reports it. In a signature,
// each unmet bound once per item, at the innermost type that needs it;
// in a bound, at the item; and again where a body or a constant's
// value normalizes its signature, unless that is where it was reported.

trait Tr {
    type Out: Copy;
}

trait Marker {}

struct Pair<A, B>(A, B);

fn same<T>(x: <T as Tr>::Out) -> (<T as Tr>::Out, <T as Tr>::Out) {
    (x, x)
}

fn ret<T>() -> <T as Tr>::Out {
    loop {}
}

fn two<T, U>(x: <T as Tr>::Out, y: Pair<<U as Tr>::Out, <T as Tr>::Out>) {}

fn nested_ret<T>() -> Pair<<T as Tr>::Out, u8> {
    loop {}
}

fn concrete(x: <i32 as Tr>::Out) {}

fn annotations<T: Copy>(x: T) {
    let a: <T as Tr>::Out = loop {};
    let b: Pair<u8, <T as Tr>::Out> = loop {};
    let c: (u8, &<T as Tr>::Out) = loop {};
}

struct Field<T> {
    x: <T as Tr>::Out,
}

struct Bounded<T>(T)
where
    Pair<<T as Tr>::Out, u8>: Marker;

pub fn bounded<T: Marker>(x: u8) -> u8
where
    Pair<<T as Tr>::Out, u8>: Marker,
{
    x
}

impl<T> Pair<T, T>
where
    Pair<<T as Tr>::Out, u8>: Marker,
{
    fn method(x: u8) -> u8 {
        x
    }
    const C: u8 = 1;
}

impl<T> Pair<T, u8> {
    fn param(self, x: <T as Tr>::Out) {}
    const D: Option<<T as Tr>::Out> = None;
}

trait Required {
    const E: <Self as Tr>::Out;
    fn f(x: <Self as Tr>::Out);
    fn g<T>(x: Pair<<T as Tr>::Out, u8>) {}
}

const K: <i32 as Tr>::Out = 5;

struct W<T: Copy>(T);

impl<T: Copy> Tr for W<T> {
    type Out = T;
}

fn through_impl<T>(x: <W<T> as Tr>::Out) {}

fn main() {}
