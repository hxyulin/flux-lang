use std::marker::PhantomData;
struct S<K: ?Sized, V: ?Sized, Store = Vec<(K, V)>>(Store, PhantomData<K>, PhantomData<V>);
fn accept(_: S<str, [u8], u32>) {}
fn main() {
    accept(S(7, PhantomData, PhantomData));
    let _: Option<S<u32, u64>> = None;
}
