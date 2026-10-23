// Independent late inputs remain reifiable through each coercion route.
fn named<'a>(x: &'a i32) -> i32 {
    *x
}
fn elided(x: &i32) -> i32 {
    *x
}
fn generic<'a, T: Copy>(x: &'a T) -> T {
    *x
}
fn static_predicate<'a>(x: &'a i32) -> i32
where
    &'static i32: Copy,
{
    *x
}
unsafe fn unsafe_identity(x: &i32) -> i32 {
    *x
}
type Pointer = fn(&i32) -> i32;
fn apply(f: Pointer, x: &i32) -> i32 {
    f(x)
}
fn main() {
    let x = 37;
    let annotation: Pointer = named;
    let cast = elided as Pointer;
    let cloned: Pointer = named.clone();
    let generic_pointer: Pointer = generic;
    let predicate_pointer: Pointer = static_predicate;
    let unsafe_pointer: unsafe fn(&i32) -> i32 = unsafe_identity;
    let safe_to_unsafe: unsafe fn(&i32) -> i32 = elided;
    println!("{} {} {} {}", annotation(&x), cast(&x), apply(named, &x), cloned(&x));
    println!("{} {} {} {}", generic_pointer(&x), predicate_pointer(&x), unsafe { unsafe_pointer(&x) }, unsafe { safe_to_unsafe(&x) });
}
