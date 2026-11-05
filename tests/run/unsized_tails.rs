//! Struct and tuple tails whose type is a ?Sized parameter.
use std::marker::PhantomData;
pub struct Wrapper<T: ?Sized>(pub T);
#[repr(C)]
pub struct HeaderSlice<H, T: ?Sized> { pub header: H, pub slice: T }
pub struct Marker<F, T: ?Sized> { marker: PhantomData<(F, T)>, n: u8 }
pub struct Adapter<T: ?Sized> { pub writeable: T }
fn ptrs<T: ?Sized>(p: *mut T) { unsafe { p.drop_in_place() } }
fn main() {
    let w = Wrapper(3u8);
    let h = HeaderSlice { header: 1u8, slice: [1u8, 2] };
    let r: &HeaderSlice<u8, [u8]> = &h;
    let m: Marker<u8, str> = Marker { marker: PhantomData, n: 4 };
    let a: &Adapter<[u8]> = &Adapter { writeable: [5u8] };
    let mut x = 9u32;
    ptrs(&mut x as *mut u32);
    println!("{} {} {} {:?}", w.0, r.slice.len(), m.n, &a.writeable);
}
