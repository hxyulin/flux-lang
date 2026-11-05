// The written lifetimes of constructor and inherent function paths bind
// the value's type exactly, then it may be a subtype.
use std::cell::Cell;

struct Inv<'a> {
    c: Cell<&'a u8>,
}

struct W<'a, T> {
    r: &'a T,
}

impl<'a, T> W<'a, T> {
    fn new(r: &'a T) -> Self {
        W { r }
    }
}

#[derive(Default)]
struct Holder<'a> {
    value: Option<&'a str>,
}

enum E<'a> {
    A(Cell<&'a u8>),
    B,
}

fn invariant_static<'a>(x: &'a u8) -> Inv<'a> {
    Inv::<'static> { c: Cell::new(x) }
}

fn invariant_widen<'a, 'b>(x: Cell<&'a u8>) -> Inv<'b> {
    Inv::<'b> { c: x }
}

fn inherent_static<'a>(x: &'a u8) -> W<'a, u8> {
    W::<'static, u8>::new(x)
}

fn inherent_argument<'a, 'b>(x: &'b &'a u8) -> W<'b, &'static u8> {
    W::<'b, &'a u8>::new(x)
}

fn trait_function<'a>(s: &'a str) -> Holder<'static> {
    let mut h = Holder::<'a>::default();
    h.value = Some(s);
    h
}

fn unit_variant<'a>() -> E<'a> {
    E::<'static>::B
}

fn variant_argument() -> bool {
    let x = 1u8;
    let o = Some::<&'static u8>(&x);
    o.is_some()
}

fn inherent_type_argument<'a>(x: &'a u8) -> usize {
    let v = Vec::<&'static u8>::from([x]);
    v.len()
}

fn vec_type_argument<'a, T>(data: &'a mut [T]) -> Vec<&'static mut [T]> {
    let mut out = Vec::<&'a mut [T]>::with_capacity(1);
    out.push(data);
    out
}

fn main() {}
