// Lifetimes written in constructor and inherent function paths: the values
// keep their regions and the program runs as rustc's does.
use std::marker::PhantomData;

struct Scope<'env> {
    data: &'env [i32],
    _marker: PhantomData<&'env mut &'env ()>,
}

impl<'env> Scope<'env> {
    fn new(data: &'env [i32]) -> Scope<'env> {
        Scope::<'env> { data, _marker: PhantomData }
    }
    fn sum(&self) -> i32 {
        self.data.iter().sum()
    }
}

struct Pair<'a, T>(&'a T, &'a T);

enum Choice<'a, T> {
    Left(&'a T),
    Right(&'a T),
    Neither,
}

fn pick<'a, T>(x: &'a T, y: &'a T, left: bool) -> Choice<'a, T> {
    if left { Choice::<'a, T>::Left(x) } else { Choice::Right::<'a, T>(y) }
}

fn none<'a>() -> Choice<'a, u8> {
    Choice::<'a, u8>::Neither
}

fn first<'a>(p: Pair<'a, i32>) -> &'a i32 {
    p.0
}

fn statics() -> Pair<'static, i32> {
    static X: i32 = 7;
    Pair::<'static, i32>(&X, &X)
}

fn with_new<'a>(data: &'a [i32]) -> i32 {
    let s = Scope::<'a>::new(data);
    s.sum()
}

fn some_ref<'a>(x: &'a u8) -> Option<&'a u8> {
    Some::<&'a u8>(x)
}

fn anon<'a>(x: &'a i32) -> Pair<'a, i32> {
    Pair::<'_, i32>(x, x)
}

#[derive(Default)]
struct Holder<'a> {
    value: Option<&'a str>,
}

fn holder<'a>() -> Holder<'a> {
    Holder::<'a>::default()
}

fn chunks<'a, T>(data: &'a mut [T]) -> Vec<&'a mut [T]> {
    let mut out = Vec::<&'a mut [T]>::with_capacity(2);
    let (a, b) = data.split_at_mut(1);
    out.push(a);
    out.push(b);
    out
}

fn nested<'a, 'b>(x: &'b &'a u8) -> W<'b, &'a u8> {
    W::<'b, &'a u8>::new(x)
}

struct W<'a, T> {
    r: &'a T,
}

impl<'a, T> W<'a, T> {
    fn new(r: &'a T) -> Self {
        W { r }
    }
}

fn main() {
    let data = [1, 2, 3];
    let s = Scope::new(&data);
    println!("{}", s.sum());
    println!("{}", with_new(&data));
    let (a, b) = (1, 2);
    match pick(&a, &b, false) {
        Choice::Left(v) => println!("left {v}"),
        Choice::Right(v) => println!("right {v}"),
        Choice::Neither => println!("neither"),
    }
    if let Choice::Neither = none() {
        println!("none");
    }
    println!("{}", first(Pair(&a, &b)));
    println!("{}", statics().1);
    println!("{:?}", some_ref(&5));
    println!("{}", anon(&9).0);
    println!("{:?}", holder().value);
    let mut v = [1, 2, 3];
    let c = chunks(&mut v);
    println!("{} {}", c.len(), c[1].len());
    let x = 4u8;
    let r = &x;
    println!("{}", **nested(&r).r);
}
