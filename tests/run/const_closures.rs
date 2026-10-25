use std::sync::LazyLock;

fn runtime(x: i32) -> i32 { x * 3 + 1 }
struct Owned(i32);
fn consume(x: Owned) -> i32 { x.0 }
static FIRST: LazyLock<Vec<i32>> = LazyLock::new(|| {
    let mut values = Vec::new();
    for n in 0..4 { values.push(runtime(n)); }
    values
});
static SECOND: LazyLock<i32> = LazyLock::new(|| runtime(8));
const PTR: fn(i32) -> i32 = |x| runtime(x);
static STATIC_PTR: fn(i32) -> i32 = |x| runtime(x) + 2;
const WRAPPED: Option<fn(i32) -> i32> = Some(|x| runtime(x) + 3);
const fn factory() -> fn(i32) -> i32 { |x| runtime(x) + 4 }
const fn captured_factory(x: i32) -> impl Fn() -> i32 { move || runtime(x) }
struct Generic<T>(T);
impl<T> Generic<T> {
    const PTR: fn(i32) -> i32 = |x| { let _ = std::mem::size_of::<T>(); runtime(x) + 5 };
}
fn main() {
    for x in FIRST.iter() { println!("{}", x); }
    println!("{} {} {} {} {} {}", *SECOND, PTR(2), STATIC_PTR(2), WRAPPED.unwrap()(2), factory()(2), Generic::<u64>::PTR(2));
    let f = const { |x: i32| runtime(x) + 6 };
    println!("{}", f(2));
    let g = const { let x = 9; move || runtime(x) };
    println!("{} {}", g(), captured_factory(12)());
    let mut counter = const { let mut n = 1; move || { n += 2; n } };
    println!("{} {}", counter(), counter());
    let once = const { let x = Owned(17); move || consume(x) };
    println!("{}", once());
}
