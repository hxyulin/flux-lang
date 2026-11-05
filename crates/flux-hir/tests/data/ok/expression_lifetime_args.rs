struct W<'a, T> {
    r: &'a T,
}

impl<'a, T> W<'a, T> {
    fn new(r: &'a T) -> Self {
        W { r }
    }
}

enum E<'a> {
    A(&'a u8),
    B,
}

fn build<'a>(x: &'a u8) -> u8 {
    let w = W::<'a, u8> { r: x };
    let n = W::<'a, u8>::new(x);
    let e = E::<'a>::A(x);
    let b = E::B::<'static>;
    let o = Some::<&'a u8>(x);
    let _ = (e, b, o);
    *w.r + *n.r
}

fn main() {
    let _ = build(&1);
}
