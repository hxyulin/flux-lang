struct S {
    a: i32,
}

struct P(i32, i32);

struct U;

struct E {}

struct D {
    a: i32,
    a: u8,
}

struct Twice {
    a: i32,
}

struct Twice(i32);

struct V;

fn V() {}

fn named_as_value() {
    let s = S;
}

fn named_called() {
    let s = S(1);
}

fn unknown_struct() {
    let n = Nope { a: 1 };
}

fn tuple_pattern_on_named(s: S) {
    let S(a) = s;
}

fn shadow_unit() {
    let mut U = 1;
}

fn shadow_tuple_struct() {
    let P = 1;
}

fn param_shadows(P: i32) {}

fn rest_twice(t: (i32, i32, i32)) {
    let (a, .., b, ..) = t;
}

fn braces_as_value() {
    let e = E;
}

fn shorthand_unknown_local() {
    let s = S { a };
}

fn main() {}
