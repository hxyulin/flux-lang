// Marker bounds the self type does not imply stay unsupported, and the
// guard's other bounds still decide whether it is a marker.
use std::cell::Cell;
use std::marker::PhantomData;

pub struct G<'a, T: ?Sized> {
    data: *mut T,
    marker: PhantomData<&'a ()>,
}

unsafe impl<'a, T: ?Sized + Sync + 'a> Sync for G<'a, T> {}

pub struct H<'a, 'b> {
    a: &'a u8,
    b: &'b u8,
}

unsafe impl<'a, 'b: 'a> Send for H<'a, 'b> {}

pub struct Mapped<'a, T: ?Sized> {
    data: *mut T,
    marker: PhantomData<&'a mut T>,
}

unsafe impl<'a, T: ?Sized + Sync + 'a> Sync for Mapped<'a, T> {}

fn is_sync<T: Sync + ?Sized>(_: &T) {}

fn main() {
    let mut c = Cell::new(1u8);
    let m = Mapped { data: &mut c as *mut Cell<u8>, marker: PhantomData };
    is_sync(&m);
}
