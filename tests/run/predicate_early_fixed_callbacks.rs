// Predicates fix own lifetimes, but fixed Param/Alias inputs remain valid.
fn identity<'a>(x: &'a i32) -> &'a i32 where &'a i32: Copy { x }
fn scalar<'a>(x: &'a i32) -> i32 where &'a i32: Sized { *x + 2 }
fn unit<'a>(x: &'a i32) where &'a i32: Copy { println!("unit {}", x); }
fn optional<'a>(x: &'a i32) -> Option<&'a i32> where &'a i32: Copy { Some(x) }
fn late<'a, T: Copy>(x: &'a T) -> &'a T { x }
fn elided(x: &i32) -> &i32 { x }
fn apply<T, U, F: Fn(T) -> U>(x: T, f: F) -> U { f(x) }
fn higher<F: Fn(&i32) -> &i32>(x: &i32, f: F) -> &i32 { f(x) }
fn first<'a>(values: &[&'a i32]) -> Option<&'a i32> {
    values.iter().copied().find_map(optional)
}
fn main() {
    let x = 41;
    println!("{} {}", apply(&x, identity), apply(&x, scalar));
    apply(&x, unit);
    println!("{} {}", [&x].into_iter().find_map(optional).unwrap(), first(&[&x]).unwrap());
    println!("{} {}", higher(&x, late), higher(&x, elided));
}
