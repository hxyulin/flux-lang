use core::cell::Cell;
use core::marker::PhantomData;
struct Wrapper<T>(T);
struct Guarded<T>(*const T);
unsafe impl<T: Sync> Send for Guarded<T> {}
struct Safe(*const u8);
impl Send for Safe {}
struct Reference;
unsafe impl Send for &Reference {}
fn send<T: Send>() {}
fn sync<T: Sync>() {}
fn missing<T>() {
    send::<Wrapper<T>>();
}
fn wrong_reference_bound<T: Send>() {
    send::<&T>();
}
fn main() {
    send::<*const u8>();
    sync::<*mut u8>();
    send::<Wrapper<*const u8>>();
    sync::<Cell<u8>>();
    send::<&Cell<u8>>();
    sync::<&mut Cell<u8>>();
    send::<PhantomData<*const u8>>();
    send::<Guarded<Cell<u8>>>();
}
