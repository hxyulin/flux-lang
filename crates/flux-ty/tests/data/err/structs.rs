struct S {
    a: i32,
}

struct Four {
    a: i32,
    b: i32,
    c: i32,
    d: i32,
}

struct Five {
    a: i32,
    b: i32,
    c: i32,
    d: i32,
    e: i32,
}

struct Two {
    a: i32,
    b: bool,
}

struct P(i32, i32);

struct Q(i32, i32);

struct U;

struct A {
    x: i32,
}

struct B {
    x: i32,
}

fn unknown_field() {
    let s = S { a: 1, b: 2 };
}

fn unknown_field_with_unassigned() {
    let s = Two { zzz: 1 };
}

fn missing_one() {
    let s = Two { a: 1 };
}

fn missing_two() {
    let s = Four { a: 1, d: 4 };
}

fn missing_three() {
    let s = Four { a: 1 };
}

fn missing_four() {
    let s = Five { a: 1 };
}

fn missing_five() {
    let s = Five {};
}

fn missing_tuple_fields() {
    let p = P {};
}

fn field_twice() {
    let s = S { a: 1, a: 2 };
}

fn no_field(s: S) {
    let n = s.b;
}

fn no_field_tuple_index() {
    let t = (1, 2);
    let n = t.5;
}

fn no_field_tuple_struct_index(p: P) {
    let n = p.2;
}

fn no_field_unit(u: ()) {
    let n = u.0;
}

fn no_field_named_on_tuple() {
    let t = (1, 2);
    let n = t.x;
}

fn no_field_unit_struct(u: U) {
    let n = u.x;
}

fn no_field_raw_pointer(p: *const S) {
    let n = p.a;
}

fn primitive_literal() {
    let n = 1.x;
}

fn primitive_i32(n: i32) {
    let m = n.x;
}

fn primitive_tuple_index(n: i32) {
    let m = n.0;
}

fn primitive_bool(b: bool) {
    let m = b.0;
}

fn primitive_nested(s: S) {
    let m = s.a.b;
}

fn tuple_struct_too_few() {
    let p = P(1);
}

fn tuple_struct_too_many() {
    let p = P(1, 2, 3);
}

fn unit_struct_called() {
    let u = U();
}

fn pattern_missing_one(s: Two) {
    let Two { a } = s;
}

fn pattern_missing_several(s: Five) {
    let Five { a } = s;
}

fn pattern_missing_in_param(Two { a }: Two) {}

fn pattern_no_such_field(s: S) {
    let S { a, z } = s;
}

fn pattern_no_such_field_rest(s: S) {
    let S { z, .. } = s;
}

fn pattern_named_on_tuple_struct(p: P) {
    let P { x: z, .. } = p;
}

fn pattern_field_twice(s: S) {
    let S { a, a: b } = s;
}

fn tuple_pattern_too_few() {
    let (a, b) = (1, 2, 3);
}

fn tuple_pattern_too_many() {
    let (a, b, c) = (1, 2);
}

fn tuple_struct_pattern_too_few(p: P) {
    let P(a) = p;
}

fn tuple_struct_pattern_too_many(p: P) {
    let P(a, b, c) = p;
}

fn tuple_struct_pattern_wrong_struct(p: P) {
    let Q(a, b) = p;
}

fn mismatched_struct_types() {
    let a: A = B { x: 1 };
}

fn takes_a(a: A) {}

fn mismatched_struct_arg() {
    takes_a(B { x: 1 });
}

fn mismatched_field_type() {
    let s = S { a: true };
}

fn mismatched_tuple_element() {
    let t: (i32, bool) = (1, 2);
}

fn mismatched_tuple_arity() {
    let t: (i32, i32) = (1, 2, 3);
}

fn one_tuple_vs_paren() {
    let t: (i32,) = (1,);
    let u: (i32,) = 1;
}

fn functional_update_wrong_type(b: B) {
    let a = A { ..b };
}

fn tuple_struct_constructor_as_value() {
    let p: P = P;
}

fn compare_structs(x: S, y: S) -> bool {
    x == y
}

fn add_tuples() {
    let t = (1, 2) + (3, 4);
}

fn main() {}
