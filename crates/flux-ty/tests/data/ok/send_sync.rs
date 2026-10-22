use core::marker::PhantomData;

struct Wrapper<T>(T);
struct Searcher<'h> {
    start: *const u8,
    haystack: PhantomData<&'h [u8]>,
}
unsafe impl<'h> Send for Searcher<'h> {}
unsafe impl<'h> Sync for Searcher<'h> {}

struct Guarded<T> { ptr: *const T }
unsafe impl<T: Sync> Send for Guarded<T> {}
unsafe impl<T: Sync> Sync for Guarded<T> {}

fn send<T: Send>(_: &T) -> usize { 1 }
fn sync<T: Sync>(_: &T) -> usize { 2 }
fn generic<T: Send + Sync>(value: T) -> usize {
    let wrapped = Wrapper(value);
    send(&wrapped) + sync(&wrapped) + send(&&wrapped)
}
fn main() {
    let bytes = [1u8, 2, 3];
    let searcher = Searcher { start: bytes.as_ptr(), haystack: PhantomData };
    let guarded = Guarded { ptr: bytes.as_ptr() };
    println!("{} {} {} {} {}", generic(42), send(&searcher), sync(&searcher), send(&guarded), sync(&guarded));
    println!("{} {}", unsafe { *searcher.start }, unsafe { *guarded.ptr });
}
