// pin-project-lite's `Unpin` impls: a lifetime only the auto trait bound
// names, and a bound that never holds but names a lifetime. A pinned
// wrapper of an `Unpin` value can be moved out of its pin.
use std::marker::{PhantomData, PhantomPinned};
use std::pin::Pin;

pub type PinnedFieldsOf<T> = <PinnedFieldsOfHelperStruct<T> as PinnedFieldsOfHelperTrait>::Actual;

pub trait PinnedFieldsOfHelperTrait {
    type Actual: ?Sized;
}

pub struct PinnedFieldsOfHelperStruct<T: ?Sized>(T);

impl<T: ?Sized> PinnedFieldsOfHelperTrait for PinnedFieldsOfHelperStruct<T> {
    type Actual = T;
}

pub struct Wrapper<'a, F: ?Sized> {
    name: &'a str,
    inner: F,
}

#[allow(dead_code)]
pub struct __Origin<'__pin, 'a, F: ?Sized> {
    __dummy_lifetime: PhantomData<&'__pin ()>,
    name: PhantomData<&'a str>,
    inner: F,
}

impl<'__pin, 'a, F: ?Sized> Unpin for Wrapper<'a, F> where
    PinnedFieldsOf<__Origin<'__pin, 'a, F>>: Unpin
{
}

pub struct NeverUnpin<F> {
    inner: F,
}

impl<'__pin, F> Unpin for NeverUnpin<F> where (PhantomData<&'__pin ()>, PhantomPinned): Unpin {}

fn take<T: Unpin>(p: Pin<&mut T>) -> &mut T {
    Pin::into_inner(p)
}

fn main() {
    let x = 3u8;
    let mut w = Wrapper { name: "pinned", inner: &x };
    let inner = take(Pin::new(&mut w));
    inner.name = "moved";
    println!("{} {}", w.name, w.inner);
    let n = NeverUnpin { inner: 7u8 };
    std::process::exit(n.inner as i32);
}
