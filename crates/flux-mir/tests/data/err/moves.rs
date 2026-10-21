struct Inner {
    x: i32,
}

struct S {
    inner: Inner,
    n: i32,
}

struct A {
    a: i32,
    b: i32,
}

fn take(_s: S) {}

fn take_inner(_i: Inner) {}

fn after_call(s: S) -> i32 {
    take(s);
    s.n
}

fn after_let(s: S) -> i32 {
    let t = s;
    take(t);
    s.n
}

fn whole_twice(s: S) {
    take(s);
    take(s);
}

fn in_loop(s: S) {
    let mut i = 0;
    while i < 3 {
        take(s);
        i += 1;
    }
}

fn in_loop_loop(s: S) {
    loop {
        take(s);
    }
}

fn field_in_loop(s: S) {
    loop {
        take_inner(s.inner);
    }
}

fn partial_then_whole(s: S) {
    let inner = s.inner;
    take_inner(inner);
    take(s);
}

fn tuple_partial_then_whole(t: (S, i32)) {
    let (s, _) = t;
    take(s);
    let u = t;
}

fn pattern_partial_then_whole(s: S) {
    let S { inner, .. } = s;
    take_inner(inner);
    take(s);
}

fn functional_update_then_whole(s: S) {
    let t = S { n: 5, ..s };
    take(t);
    take(s);
}

fn same_field_twice(s: S) {
    take_inner(s.inner);
    take_inner(s.inner);
}

fn moved_field_then_subfield(s: S) -> i32 {
    take_inner(s.inner);
    s.inner.x
}

fn whole_then_copy_field(s: S) -> i32 {
    take(s);
    s.n
}

fn tuple_of_struct(t: (S, i32)) -> i32 {
    let u = t;
    t.1
}

fn assign_field_not_mut() {
    let s = A { a: 1, b: 2 };
    s.a = 5;
}

fn compound_assign_tuple_not_mut() {
    let t = (1, 2);
    t.0 += 1;
}

fn assign_field_of_param(s: A) {
    s.a = 2;
}

fn partially_assigned() {
    let s: A;
    s.a = 1;
}

fn partially_assigned_mut() {
    let mut s: A;
    s.a = 1;
    s.b = 2;
}

fn read_uninit_field() -> i32 {
    let s: A;
    s.a
}

fn read_uninit_whole() {
    let s: S;
    take(s);
}

fn assign_to_part_of_moved(mut s: S) {
    take(s);
    s.n = 1;
}

fn main() {}
