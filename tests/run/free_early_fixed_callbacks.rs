// Bounded free function items still accept fixed Param and Alias inputs.
fn identity<'a: 'a>(x: &'a i32) -> &'a i32 { x }
fn scalar<'a: 'a>(x: &'a i32) -> i32 { *x + 1 }
fn unit<'a: 'a>(x: &'a i32) { println!("unit {}", x); }
fn optional<'a: 'a>(x: &'a i32) -> Option<&'a i32> { Some(x) }
fn late<'a>(x: &'a i32) -> &'a i32 { x }
fn elided(x: &i32) -> &i32 { x }
fn apply<T, U, F: Fn(T) -> U>(x: T, f: F) -> U { f(x) }
fn higher<F: Fn(&i32) -> &i32>(x: &i32, f: F) -> &i32 { f(x) }
fn first<'a>(values: &[&'a i32]) -> Option<&'a i32> {
    values.iter().copied().find_map(optional)
}
fn main() {
    let x = 37;
    println!("{} {}", apply(&x, identity), apply(&x, scalar));
    apply(&x, unit);
    println!("{} {}", [&x].into_iter().find_map(optional).unwrap(), first(&[&x]).unwrap());
    println!("{} {}", higher(&x, late), higher(&x, elided));
}
