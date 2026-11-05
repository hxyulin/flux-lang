struct W<'a, T> {
    r: &'a T,
}

impl<'a, T> W<'a, T> {
    const N: usize = 1;
    fn get<'b>(&self, x: &'b u8) -> &'b u8 {
        x
    }
}

trait Tr<'a> {
    fn f() -> u8;
}

impl<'a> Tr<'a> for u8 {
    fn f() -> u8 {
        1
    }
}

fn g<'a: 'a>() {}

fn too_many<'a>(x: &'a u8) -> W<'a, u8> {
    W::<'a, 'a, u8> { r: x }
}

fn unsupported<'a>(w: W<'a, u8>, x: &'a u8) -> usize {
    let _ = w.get::<'a>(x);
    g::<'a>();
    let _ = <u8 as Tr<'a, 'a>>::f();
    let h = <u8 as Tr<'a>>::f;
    let _ = h();
    let W::<'a, u8> { r: _ } = w;
    let f = Some::<&'a u8>;
    let _ = f(x);
    W::<'a, u8>::N
}

fn main() {}
