// Fixed item slots remain usable through type parameters and Iterator::Item.
struct S<'a>(&'a i32);
impl<'a> S<'a> {
    fn get(x: &'a i32) -> &'a i32 { x }
    fn cast(x: &'a i32) -> Option<Self> { Some(S(x)) }
}
fn apply<T, F: Fn(T) -> T>(x: T, f: F) -> T { f(x) }
fn identity(x: &i32) -> &i32 { x }
fn higher<F: for<'a> Fn(&'a i32) -> &'a i32>(f: F, x: &i32) -> &i32 { f(x) }
fn cast_first<'a>(values: &[&'a i32]) -> Option<S<'a>> {
    values.iter().copied().find_map(S::cast)
}
fn main() {
    let x = 37;
    println!("{}", apply(&x, S::get));
    println!("{}", [&x].into_iter().find_map(S::cast).unwrap().0);
    println!("{}", cast_first(&[&x]).unwrap().0);
    println!("{}", higher(identity, &x));
}
