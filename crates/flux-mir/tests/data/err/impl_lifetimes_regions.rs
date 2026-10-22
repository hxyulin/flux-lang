// Trait `impl` blocks chosen by types, whose lifetimes then do not work
// out, as rustc's borrow checker reports: `impl Tr for &'static S` used
// with a borrow of a local (E0597, through a method call or a generic
// bound) or of a parameter (E0521, "borrowed data escapes outside of
// function"); a lifetime named twice in the header of an invariant
// type's impl, each way round ("lifetime may not live long enough");
// and what bounds say associated types are (`I: Iterator<Item = &'a
// i32>`), which callers must meet (E0597, E0515, E0521) and the body
// may not stretch; and a copy of a type that is `Copy` for `'static`
// only.

trait Tr {
    fn get(self) -> i32;
}

struct S(i32);

impl Tr for &'static S {
    fn get(self) -> i32 {
        self.0
    }
}

fn method_on_local() -> i32 {
    let s = S(1);
    let r = &s;
    r.get()
}

fn need<T: Tr>(t: T) -> i32 {
    t.get()
}

fn bound_on_local() -> i32 {
    let s = S(2);
    need(&s)
}

fn param<'a>(x: &'a S) -> i32 {
    x.get()
}

struct Inv<'a> {
    p: *mut &'a i32,
}

impl<'a> PartialEq for Inv<'a> {
    fn eq(&self, _o: &Inv<'a>) -> bool {
        true
    }
}

fn invariant<'a, 'b>(x: Inv<'a>, y: Inv<'b>) -> bool {
    x == y
}

fn first<'a, I: Iterator<Item = &'a i32>>(mut it: I) -> Option<&'a i32> {
    it.next()
}

fn too_long<'a, I: Iterator<Item = &'a i32>>(mut it: I) -> &'static i32 {
    it.next().unwrap()
}

fn returns_local() -> &'static i32 {
    let v = [1, 2];
    first(v.iter()).unwrap()
}

fn need_static<I: Iterator<Item = &'static i32>>(_it: I) {}

fn local_to_static() {
    let v = [1];
    need_static(v.iter());
}

fn param_to_static<'b>(v: &'b [i32]) {
    need_static(v.iter());
}

struct X<'a>(&'a i32);

impl Clone for X<'static> {
    fn clone(&self) -> Self {
        X(self.0)
    }
}

impl Copy for X<'static> {}

fn copied<'a>(x: X<'a>) -> i32 {
    let y = x;
    *x.0 + *y.0
}

fn main() {}
