static VALUE: i32 = 41;
struct Hold<'a, T> {
    value: &'a T,
}
type StaticRef<T> = &'static T;
type Nested<T> = (T,);
type Explicit<'a, T> = &'a T;
fn choose(yes: bool) -> &'static i32 {
    let r: &'static i32;
    if yes { r = &VALUE; } else { r = &42; }
    r
}
fn generic<T>(x: &'static T) -> &'static T {
    let r: StaticRef<T> = x;
    r
}
fn main() {
    let r: &'static i32 = &VALUE;
    let slice: &'static [i32] = &[1, 2, 3];
    let inferred: &'static _ = &VALUE;
    let wrapped: Nested<&'static i32> = (r,);
    let explicit: Explicit<'static, i32> = r;
    let implicit: StaticRef<i32> = explicit;
    let hold: Hold<'static, i32> = Hold { value: implicit };
    let (a, b): (&'static i32, &'static i32);
    a = choose(true);
    b = generic(&VALUE);
    let (mut c,): (&'static i32,) = (r,);
    c = &42;
    let Hold { value: d }: Hold<'static, i32> = hold;
    let [e]: [&'static i32; 1] = [r];
    let ref f: &'static i32 = r;
    let _ = (slice, inferred, wrapped, a, b, c, d, e, f);
}
fn mixed(x: &i32) -> i32 {
    let mut pair: (&'static i32, &i32) = (&VALUE, &VALUE);
    pair.1 = x;
    *pair.0 + *pair.1
}
fn implied<T>(x: &'static &T) {
    let _: &'static &'static T = x;
}
fn inferred_inner(x: &&'static i32) {
    let _: &&'static _ = x;
}
struct DefaultRef<T = &'static i32> { value: T }
struct DefaultCopy<T, U = T> { first: T, second: U }
fn defaults() {
    let _: DefaultRef = DefaultRef { value: &VALUE };
    let _: DefaultCopy<&'static i32> = DefaultCopy { first: &VALUE, second: &42 };
}
type Inferred<T> = (T, &'static i32);
fn alias_placeholder(x: &i32) {
    let _: Inferred<_> = (x, &VALUE);
}
fn let_else() {
    let Some(r): Option<&'static i32> = Some(&VALUE) else { return; };
    let _: &'static i32 = r;
}
fn closure() {
    let f = || { let r: &'static i32 = &VALUE; *r };
    let _ = f();
}
fn tuple_placeholder(x: &i32) {
    let _: (_, &'static i32) = (x, &VALUE);
}
