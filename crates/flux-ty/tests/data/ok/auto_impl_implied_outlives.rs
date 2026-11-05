// lock_api's guard markers: `R: 'a` and `T: 'a` follow from the guards'
// fields, so every instance of the self type meets them.
use std::cell::UnsafeCell;
use std::marker::PhantomData;

pub trait RawMutex {
    type GuardMarker;
}

pub struct Raw;

impl RawMutex for Raw {
    type GuardMarker = ();
}

pub struct Mutex<R, T: ?Sized> {
    raw: R,
    data: UnsafeCell<T>,
}

pub struct MutexGuard<'a, R: RawMutex, T: ?Sized> {
    mutex: &'a Mutex<R, T>,
    marker: PhantomData<(&'a mut T, R::GuardMarker)>,
}

unsafe impl<'a, R: RawMutex + Sync + 'a, T: ?Sized + Sync + 'a> Sync for MutexGuard<'a, R, T> {}

pub struct MappedGuard<'a, R, T: ?Sized> {
    raw: &'a R,
    data: *mut T,
    marker: PhantomData<&'a mut T>,
}

unsafe impl<'a, R: Sync + 'a, T: ?Sized + Sync + 'a> Sync for MappedGuard<'a, R, T> {}

unsafe impl<'a, R: Sync + 'a, T: ?Sized + Send + 'a> Send for MappedGuard<'a, R, T> where R: 'a {}

fn is_sync<T: Sync + ?Sized>(_: &T) -> bool {
    true
}

fn is_send<T: Send + ?Sized>(_: &T) -> bool {
    true
}

fn generic<'a, T: Sync + Send>(g: &MappedGuard<'a, u8, T>) -> bool {
    is_sync(g) && is_send(g)
}

fn main() {
    let m = Mutex { raw: Raw, data: UnsafeCell::new(5u8) };
    let g = MutexGuard { mutex: &m, marker: PhantomData };
    let mut x = 1u8;
    let raw = 0u8;
    let mg = MappedGuard { raw: &raw, data: &mut x as *mut u8, marker: PhantomData };
    let _ = (is_sync(&g), generic(&mg), &m.raw, mg.raw, g.mutex.data.get(), mg.data);
}
