// Moves where a copy might be expected: structs that are `Clone` but not
// `Copy`, type parameters without a `Copy` bound (even with `Clone`),
// derived `Copy` on a generic struct whose argument is not `Copy`, and
// mutable references, which are moved. Moving out of a reference (E0507),
// by `*`, by field or by a method taking `self`, is an error unless the
// type is `Copy`; so is using a struct after moving one of its fields.

#[derive(Clone)]
struct Owned(i32);

#[derive(Clone, Copy)]
struct Pair<T>(T, T);

struct Holder {
    a: Owned,
    n: i32,
}

impl Owned {
    fn consume(self) -> i32 {
        self.0
    }
}

fn take<T>(_t: T) {}

// `Clone` alone does not make a type `Copy`.
fn clone_not_copy(o: Owned) {
    take(o);
    take(o);
}

// Without a `Copy` bound a parameter is moved, `Clone` or not.
fn param<T>(t: T) -> (T, T) {
    (t, t)
}

fn param_clone<T: Clone>(t: T) -> (T, T) {
    let u = t;
    (t, u)
}

// Derived `Copy` needs the argument to be `Copy`.
fn generic_arg(p: Pair<Owned>) {
    take(p);
    take(p);
}

fn generic_param<T>(p: Pair<T>) {
    let _q = p;
    let _r = p;
}

// Mutable references are moved into a generic parameter.
fn mut_ref(m: &mut i32) {
    take(m);
    take(m);
}

// E0507: moving out of a reference.
fn deref(r: &Owned, m: &mut Owned) {
    let _a = *r;
    let _b = *m;
}

fn field_of_ref(h: &Holder) -> i32 {
    let _a = h.a;
    h.n
}

fn generic_deref<T>(r: &T, p: &Pair<Owned>) {
    let _a = *r;
    let _b = *p;
}

fn method_through_ref(r: &Owned, h: &Holder) -> i32 {
    r.consume() + h.a.consume()
}

// A partially moved struct cannot be used whole, nor the moved field.
fn partial(h: Holder) -> Holder {
    take(h.a);
    let _n = h.n;
    h
}

fn partial_field(h: Holder) -> i32 {
    let _a = h.a;
    h.a.consume()
}

// A move in a loop moves on every iteration.
fn in_loop(o: Owned) {
    let mut i = 0;
    while i < 2 {
        take(o);
        i += 1;
    }
}

// `Copy` impls whose bounds need each other: neither holds, as in rustc.
struct X<T>(*const T);

struct Y<T>(*const T);

impl<T> Clone for X<T> {
    fn clone(&self) -> Self {
        X(self.0)
    }
}

impl<T> Clone for Y<T> {
    fn clone(&self) -> Self {
        Y(self.0)
    }
}

impl<T> Copy for X<T> where Y<T>: Copy {}

impl<T> Copy for Y<T> where X<T>: Copy {}

fn cyclic(x: X<i32>) -> (X<i32>, X<i32>) {
    (x, x)
}

fn main() {}
