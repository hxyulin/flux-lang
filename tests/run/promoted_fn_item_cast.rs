use std::marker::PhantomData;
use std::ops::Deref;

struct Factory<T>(PhantomData<T>);

fn make<T>() -> usize {
    std::mem::size_of::<T>()
}

impl<T> Deref for Factory<T> {
    type Target = fn() -> usize;

    fn deref(&self) -> &Self::Target {
        &(make::<T> as fn() -> usize)
    }
}

fn main() {
    let small = Factory::<u8>(PhantomData);
    let large = Factory::<u64>(PhantomData);
    println!("{} {}", small(), large());
}
