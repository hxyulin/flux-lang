// Lifetimes that rustc rejects: undeclared names, elision that finds
// nothing, wrong numbers of arguments and unused parameters.

struct S<'a, T> {
    x: &'a T,
}

struct Q<'a> {
    x: &'a i32,
}

struct Two<'a, 'b> {
    x: &'a i32,
    y: &'b i32,
}

fn undeclared(x: &'b i32) {}

fn both(x: &i32, y: &i32) -> &i32 {
    loop {}
}

fn three(x: &i32, y: &i32, z: &i32) -> &i32 {
    loop {}
}

fn nothing() -> &i32 {
    loop {}
}

fn no_reference(x: i32) -> &i32 {
    loop {}
}

fn named_twice<'a>(x: &'a i32, y: &'a i32) -> &i32 {
    loop {}
}

fn hidden_two(x: Two) -> &i32 {
    loop {}
}

fn pattern((x, y): (&i32, &i32)) -> &i32 {
    loop {}
}

struct Field {
    x: &i32,
}

struct Hidden {
    x: S<i32>,
}

struct Bare {
    x: Q,
}

struct Anonymous<'a> {
    x: &'_ i32,
    y: &'a i32,
}

struct Unused<'a>(i32);

fn too_many(x: Q<'static, 'static>) {}

type Alias = &i32;

fn outer<'a>(x: &'a i32) {
    fn inner(y: &'a i32) {}
    let z: &'b i32 = x;
}

fn bad_bound<T: 'b>() {}

fn where_bound<'a, T>()
where
    'a: 'c,
    T: 'a,
{
}

fn elided_in_bound<T: PartialEq<&i32>>(x: T) {}

fn main() {}
