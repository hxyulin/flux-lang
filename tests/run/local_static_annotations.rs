static VALUE: i32 = 41;
struct Hold<'a, T> { value: &'a T }
type StaticRef<T> = &'static T;
type Wrapped<T> = (T,);

fn pick(yes: bool) -> &'static i32 {
    let r: &'static i32;
    if yes { r = &VALUE; } else { r = &42; }
    r
}
fn generic<T>(x: &'static T) -> &'static T {
    let r: StaticRef<T> = x;
    r
}
fn mixed(x: &i32) -> i32 {
    let mut pair: (&'static i32, &i32) = (&VALUE, &VALUE);
    pair.1 = x;
    *pair.0 + *pair.1
}
fn main() {
    let slice: &'static [i32] = &[1, 2, 3];
    let inferred: &'static _ = &VALUE;
    let wrapped: Wrapped<&'static i32> = (inferred,);
    let hold: Hold<'static, i32> = Hold { value: generic(wrapped.0) };
    let Hold { value: r }: Hold<'static, i32> = hold;
    let (a, b): (&'static i32, &'static i32);
    a = pick(true);
    b = pick(false);
    let [c]: [&'static i32; 1] = [r];
    let ref d: &'static i32 = r;
    let x = 7;
    println!("{} {} {} {} {} {}", slice[1], *a, *b, *c, **d, mixed(&x));
}
