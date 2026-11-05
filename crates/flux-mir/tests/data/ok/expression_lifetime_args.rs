// Lifetimes written in constructor and inherent function paths.
use std::marker::PhantomData;

struct Scope<'env> {
    data: &'env [i32],
    _marker: PhantomData<&'env mut &'env ()>,
}

impl<'env> Scope<'env> {
    fn new(data: &'env [i32]) -> Scope<'env> {
        Scope::<'env> { data, _marker: PhantomData }
    }
}

struct Pair<'a, T>(&'a T, &'a T);

enum Choice<'a, T> {
    Left(&'a T),
    Neither,
}

fn pick<'a, T>(x: &'a T) -> Choice<'a, T> {
    Choice::<'a, T>::Left(x)
}

fn neither<'a>() -> Choice<'a, u8> {
    Choice::<'a, u8>::Neither
}

fn statics() -> Pair<'static, i32> {
    static X: i32 = 7;
    Pair::<'static, i32>(&X, &X)
}

fn shorter<'a>(x: &'static u8) -> Option<&'a u8> {
    Some::<&'static u8>(x)
}

fn with_new<'a>(data: &'a [i32]) -> Scope<'a> {
    Scope::<'a>::new(data)
}

fn chunks<'a, T>(data: &'a mut [T]) -> Vec<&'a mut [T]> {
    let mut out = Vec::<&'a mut [T]>::with_capacity(1);
    out.push(data);
    out
}

fn anon<'a>(x: &'a i32) -> Pair<'a, i32> {
    Pair::<'_, i32>(x, x)
}

fn covariant_result() -> usize {
    let x = 1u8;
    let mut v = Vec::<&'static u8>::new();
    v.push(&x);
    v.len()
}

struct Input<'a> {
    data: &'a [u8],
}

trait Arb<'a>: Sized {
    fn arb(u: &mut Input<'a>) -> Self;
}

impl<'a> Arb<'a> for u8 {
    fn arb(u: &mut Input<'a>) -> Self {
        u.data[0]
    }
}

fn qualified<'a>(u: &mut Input<'a>) -> u8 {
    let first: u8 = Arb::<'a>::arb(u);
    first + <u8 as Arb<'a>>::arb(u)
}

fn generic<'a, A: Arb<'a>>(u: &mut Input<'a>) -> A {
    <A as Arb<'a>>::arb(u)
}

fn main() {
    let bytes = [1u8];
    let _ = qualified(&mut Input { data: &bytes });
    let _: u8 = generic(&mut Input { data: &bytes });
    let data = [1, 2, 3];
    let _ = Scope::new(&data);
    let _ = with_new(&data);
    let _ = pick(&1);
    let _ = neither();
    let _ = statics();
    let _ = shorter(&2);
    let mut v = [1, 2];
    let _ = chunks(&mut v);
    let _ = anon(&3);
    let _ = covariant_result();
}
