use core::marker::PhantomData;

trait Input {}
impl Input for u8 {}

trait Describe {}
impl<T: Describe + ?Sized> Describe for &mut T {}
impl<A: Input, R> Describe for dyn FnMut(A) -> R + '_ {}

trait Callback: Describe {}
impl<A: Input, R> Callback for dyn FnMut(A) -> R + '_ {}

struct Scoped<'a, T: ?Sized>(PhantomData<&'a T>);

trait Convert {
    type Abi;
}

impl<A, R> Convert for &'_ mut (dyn FnMut(A) -> R + '_)
where
    Self: Describe,
{
    type Abi = u64;
}

impl<T: Callback + ?Sized> Convert for &Scoped<'_, T> {
    type Abi = u32;
}

trait Element {
    type Value;
}

impl<T> Element for (T,) {
    type Value = T;
}

impl Convert for &u32 {
    type Abi = u8;
}

fn identity<T>(value: T) -> T {
    value
}

fn main() {
    let direct = identity::<<&'static mut dyn FnMut(u8) -> u32 as Convert>::Abi>(37);
    let scoped = identity::<<&'static Scoped<'static, dyn FnMut(u8) -> u32> as Convert>::Abi>(41);
    let nested = identity::<<&'static <(u32,) as Element>::Value as Convert>::Abi>(43);
    println!("{direct} {scoped} {nested}");
}
