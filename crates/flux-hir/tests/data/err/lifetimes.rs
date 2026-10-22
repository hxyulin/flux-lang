// Lifetime parameters and arguments that rustc rejects before resolving
// names.

struct Misordered<T, 'a> {
    x: &'a T,
}

fn duplicate<'a, 'a>(x: &'a i32) {}

fn reserved<'static>(x: &'static i32) {}

fn anonymous<'_>() {}

struct S<'a, T> {
    x: &'a T,
}

fn type_first(x: S<i32, 'static>) {}

struct Q<'a> {
    x: &'a i32,
}

impl<'a> Q<'a> {
    fn shadows<'a>(&self) {}
}

fn bounds<'a: 'a, T: 'a + Copy>(x: &'a T) where 'a: 'a, T: 'a {}

fn main() {}
