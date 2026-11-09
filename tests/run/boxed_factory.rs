use std::marker::PhantomData;
use std::panic::{RefUnwindSafe, UnwindSafe};

struct Cache(u32);
struct PoolGuard<'a, T: Send, F: Fn() -> T> {
    pool: &'a (),
    factory: F,
    marker: PhantomData<T>,
}
type Factory<'a> = Box<dyn Fn() -> Cache + Send + Sync + UnwindSafe + RefUnwindSafe + 'a>;
struct Matches<'a> { guard: PoolGuard<'a, Cache, Factory<'a>> }

fn make<'a>(pool: &'a (), value: &'a u32) -> Matches<'a> {
    let factory: Factory<'a> = Box::new(move || Cache(*value));
    Matches { guard: PoolGuard { pool, factory, marker: PhantomData } }
}
fn main() {
    let pool = ();
    let value = 42;
    let matches = make(&pool, &value);
    let Cache(result) = (matches.guard.factory)();
    assert_eq!(matches.guard.pool, &());
    assert_eq!(result, 42);
    println!("{result}");
}
