// The erased auto trait bounds still decide the impl, and lifetimes that
// other bounds name stay unsupported.
use std::marker::{PhantomData, PhantomPinned};

pub type PinnedFieldsOf<T> = <PinnedFieldsOfHelperStruct<T> as PinnedFieldsOfHelperTrait>::Actual;

pub trait PinnedFieldsOfHelperTrait {
    type Actual: ?Sized;
}

pub struct PinnedFieldsOfHelperStruct<T: ?Sized>(T);

impl<T: ?Sized> PinnedFieldsOfHelperTrait for PinnedFieldsOfHelperStruct<T> {
    type Actual = T;
}

pub struct Wrapper<F> {
    inner: F,
}

#[allow(dead_code)]
pub struct __Origin<'__pin, F> {
    __dummy_lifetime: PhantomData<&'__pin ()>,
    inner: F,
}

impl<'__pin, F> Unpin for Wrapper<F> where PinnedFieldsOf<__Origin<'__pin, F>>: Unpin {}

pub struct NeverUnpin<F> {
    inner: F,
}

impl<'__pin, F> Unpin for NeverUnpin<F> where (PhantomData<&'__pin ()>, PhantomPinned): Unpin {}

trait Tr {}

impl Tr for &'static u8 {}

pub struct ByTrait<T>(T);

unsafe impl<'p, T> Send for ByTrait<T> where &'p u8: Tr {}

pub struct ByOutlives<T>(T);

unsafe impl<'p, T> Sync for ByOutlives<T> where 'p: 'static, T: 'p {}

fn is_unpin<T: Unpin>(_: &T) {}

fn main() {
    is_unpin(&Wrapper { inner: PhantomPinned });
    is_unpin(&NeverUnpin { inner: 1u8 });
}
