// pin-project-lite's `Unpin` impls: a lifetime that only the auto trait
// bounds name, and a bound that never holds but names a lifetime.
use std::marker::{PhantomData, PhantomPinned};

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

fn is_unpin<T: Unpin + ?Sized>(_: &T) -> bool {
    true
}

fn generic<F: Unpin>(w: &Wrapper<F>) -> bool {
    is_unpin(w)
}

fn main() {
    let x = 3u8;
    let w = Wrapper { name: "w", inner: &x };
    let _ = (is_unpin(&w), generic(&w), w.name, *w.inner);
    let n = NeverUnpin { inner: 1u8 };
    let _ = n.inner;
}
