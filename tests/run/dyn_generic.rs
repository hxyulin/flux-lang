//! Trait objects made in generic code, whose vtables are known once it
//! is monomorphized: generic functions erasing `&T` to `&dyn Tr`, slices
//! of them, and trait objects of types that hold trait objects.

unsafe extern "C" { fn printf(format: *const i8, ...) -> i32; }
trait Tr { fn get(&self) -> i32; }
impl Tr for i32 { fn get(&self) -> i32 { *self } }
impl Tr for u8 { fn get(&self) -> i32 { *self as i32 * 100 } }
impl<T: Tr> Tr for (T, T) { fn get(&self) -> i32 { self.0.get() + self.1.get() } }
fn erase<T: Tr>(x: &T) -> &dyn Tr { x }
fn erase_all<'a, T: Tr>(xs: &'a [T], out: &mut [Option<&'a dyn Tr>]) {
    for (i, x) in xs.iter().enumerate() { out[i] = Some(x); }
}
struct Wrap<'a> { inner: &'a dyn Tr }
impl Tr for Wrap<'_> { fn get(&self) -> i32 { self.inner.get() + 1 } }
fn main() {
    let a = 5i32;
    let b = 7u8;
    let c = (1i32, 2i32);
    let xs = [3u8, 4];
    let mut out: [Option<&dyn Tr>; 2] = [None, None];
    erase_all(&xs, &mut out);
    let w = Wrap { inner: &c };
    let ww = Wrap { inner: &w };
    let e = erase(&ww);
    unsafe {
        printf(c"%d %d %d %d %d\n".as_ptr(), erase(&a).get(), erase(&b).get(), erase(&c).get(), out[1].unwrap().get(), e.get());
    }
}
