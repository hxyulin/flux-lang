use std::marker::PhantomData;

trait Read<'data> {
    fn read(&self) -> &'data u32;
}
impl<'data> Read<'data> for &'data u32 {
    fn read(&self) -> &'data u32 {
        *self
    }
}
struct Cache<'data, R: Read<'data>> {
    reader: R,
    marker: PhantomData<&'data ()>,
}
impl<'data, R: Read<'data>> Cache<'data, R> {
    fn data(&self) -> &'data u32 {
        self.reader.read()
    }
    fn apply<B, F>(self, initial: B, mut callback: F) -> B
    where
        Self: Sized,
        F: FnMut(B, &'data u32) -> B,
    {
        callback(initial, self.reader.read())
    }
}
fn cache<'data, R: Read<'data>>(reader: R) -> Cache<'data, R> {
    Cache {
        reader,
        marker: PhantomData,
    }
}
struct Nested<'data, R: Read<'data>> {
    cache: Cache<'data, R>,
}
type Borrowed<'data> = Cache<'data, &'data u32>;

trait Echo<T> {
    fn echo(self, value: T) -> T;
}
impl<T> Echo<T> for () {
    fn echo(self, value: T) -> T {
        value
    }
}
struct Typed<T, R: Echo<T>> {
    reader: R,
    marker: PhantomData<T>,
}
fn echo<'data, R: Echo<&'data u32>>(reader: R, value: &'data u32) -> &'data u32 {
    let typed: Typed<&u32, R> = Typed {
        reader,
        marker: PhantomData,
    };
    typed.reader.echo(value)
}
fn main() {
    let first = 17u32;
    let second = 21u32;
    let outer = Nested {
        cache: cache(&first),
    };
    let stored = outer.cache.data();
    let alias: Borrowed<'_> = cache(&second);
    let optional: Option<Cache<'_, &u32>> = Some(cache(&first));
    assert_eq!(*stored, 17);
    assert_eq!(*alias.data(), 21);
    assert_eq!(*optional.unwrap().data(), 17);
    assert_eq!(*echo((), &second), 21);
    assert_eq!(cache(&first).apply(4, |total, value| total + *value), 21);
    println!("adt {} {}", stored, alias.data());
}
