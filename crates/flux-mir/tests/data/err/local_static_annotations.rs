struct Hold<'a, T> { value: &'a T }
type StaticRef<T> = &'static T;
type Wrapped<T> = (T,);
type Explicit<'a, T> = &'a T;

fn direct() {
    let x = 1;
    let _r: &'static i32 = &x;
}
fn parameter(x: &i32) {
    let _r: &'static i32 = x;
}
fn nested(x: &i32) {
    let _r: &&'static i32 = &x;
}
fn nested_outer(x: &&i32) {
    let _r: &'static &i32 = x;
}
fn aggregate(x: &i32) {
    let _r: (i32, [&'static i32; 1]) = (1, [x]);
}
fn adt_lifetime(x: &i32) {
    let _s: Hold<'static, i32> = Hold { value: x };
}
fn adt_type(x: &i32) {
    let _t: Hold<'_, &'static i32> = Hold { value: &x };
}
fn alias_static(x: &i32) {
    let _r: StaticRef<i32> = x;
}
fn alias_type(x: &i32) {
    let _s: Wrapped<&'static i32> = (x,);
}
fn alias_lifetime(x: &i32) {
    let _t: Explicit<'static, i32> = x;
}
fn later(x: &i32, yes: bool) {
    let r: &'static i32;
    if yes { r = x; } else { r = &0; }
    let _ = r;
}
fn reassign(x: &i32) {
    let mut r: &'static i32 = &0;
    r = x;
    let _ = r;
}
fn destructured(x: &i32) {
    let (a, b): (&'static i32, &'static i32);
    a = x;
    b = &0;
    let _ = (a, b);
}
fn wildcard(x: &i32) {
    let _: &'static i32 = x;
}
fn wild_tuple(x: &i32) {
    let (_, _): (&'static i32, i32) = (x, 0);
}
fn pattern(x: &i32) {
    let (mut a,): (&'static i32,) = (&0,);
    a = x;
    let _ = a;
}
fn record_pattern(x: &i32) {
    let Hold { value: mut b }: Hold<'static, i32> = Hold { value: &0 };
    b = x;
    let _ = b;
}
fn branch(x: &i32, yes: bool) {
    let _r: &'static i32 = if yes { &0 } else { x };
}
fn generic_wf<T>() {
    let _: Option<&'static T> = None;
}
fn nested_adt_wf<T>() {
    let _: Option<Hold<'static, T>> = None;
}
fn closure(x: &i32) {
    let f = || { let _: &'static i32 = x; };
    f();
}
fn main() {}
fn captured(x: &i32) {
    let f = || { let _r: &'static i32 = x; };
    f();
}
struct DefaultRef<T = &'static i32> { value: T }
struct DefaultCopy<T, U = T> { first: T, second: U }
fn default_static(x: &i32) {
    let _: DefaultRef = DefaultRef { value: x };
}
fn default_substitute(x: &i32) {
    let _: DefaultCopy<&'static i32> = DefaultCopy { first: &0, second: x };
}
type Inferred<T> = (T, &'static i32);
fn alias_placeholder(x: &i32) {
    let _: Inferred<_> = (0, x);
}
fn mutable() {
    let mut x = 1;
    let _: &'static mut i32 = &mut x;
}
fn local_slice() {
    let a = [1, 2];
    let _: &'static [i32] = &a;
}
enum Choice<'a> { Some(&'a i32), None }
fn enum_lifetime(x: &i32) {
    let _: Choice<'static> = Choice::Some(x);
}
fn match_branch(x: &i32, yes: bool) {
    let _: &'static i32 = match yes { true => &0, false => x };
}
fn tuple_placeholder(x: &i32) {
    let _: (_, &'static i32) = (&0, x);
}
struct Bound<'a, T: 'static> { lifetime: &'a (), value: T }
fn static_adt_bound(x: &i32) {
    let _: Bound<'static, &i32> = Bound { lifetime: &(), value: x };
}
struct Related<'a: 'b, 'b> { first: &'a i32, second: &'b i32 }
fn adt_outlives(x: &i32) {
    let _: Related<'_, 'static> = Related { first: x, second: &0 };
}
